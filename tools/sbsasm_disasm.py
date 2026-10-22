#!/usr/bin/env python3
"""
sbsasm_disasm.py -- best-effort annotated disassembler for Adobe Substance
"compiled assembly" (.sbsasm, SBAM) bytecode.

PROVENANCE / LEGAL NOTE
-----------------------
This tool was written by inspecting *container-level* structure of files the
user already lawfully extracted from .sbsar / .spsm archives.  It contains no
Adobe source code.  All opcode semantics below are derived from statistical
diffing of one sample (mg_dust.sbsasm, 9060 bytes) cross-referenced against
its companion sbsdescription XML (mg_dust.xml).  Every conclusion is graded
"confirmed" / "observed" / "hypothesis" in docs/SBSASM_BYTECODE_NOTES.md.

USAGE
-----
    python3 tools/sbsasm_disasm.py <file.sbsasm> [--xml sibling.xml] [--verbose]
        --xml      optional: an sbsdescription XML next to the file; its
                   input uid=".." attributes are used to name parameter
                   references (uid bytes are embedded in the bytecode).
        --verbose  also list every decoded record/anchor, not just summary.

WHAT IT DOES
------------
1. Parses the confirmed SBAM header fields and the trailing descriptor table.
2. Locates section boundaries heuristically (record markers).
3. Walks the opcode stream with a table-driven decoder:
        instruction := u16 LE head  +  N u16 LE operands,
        head = [param:u8][op:u8]  (little-endian: param byte first, op byte 2nd)
   arity table (HYPOTHESIS, resync-tolerant):
        op 0x00 -> 0 args   (boundary / nop-like)
        op 0x05 -> 1 arg    (unary-ish)
        op 0x08 -> 2 args   (binary-ish)
        op 0x09 -> 2 args   (ALU/select-ish); param 0x02 -> takes a u32 LE
                    parameter UID instead (confirmed against the companion XML)
        op 0x0d -> 2 args (param 0) else 3 args (compare/select-ish)
        op 0x0a -> 2 u16 words forming a u32 (uid reference when param==2)
   On a head that is not a known op it resynchronises forward byte-by-byte and
   counts the skipped bytes as UNKNOWN (this is where the tool can be wrong;
   it is labelled per-site).
4. Emits: file map, record table, disassembly per block, opcode histogram,
   unknown-run histogram, and any u32 parameter UIDs seen (with XML names).

Exit code 0 on success.
"""

import argparse
import collections
import os
import struct
import sys

# ----------------------------------------------------------------------------
# f16 oddly useful constants that appear as raw u16 operands (observed)
F16 = {0x3F80: "1.0", 0x3E80: "0.25", 0x3F00: "0.5",
       0x4000: "2.0", 0x4100: "8.0", 0x0000: "0.0"}

RECORD_U32 = set()      # filled dynamically: seen record marker words

# arity table per op byte (in u16 operands) -- HYPOTHESIS, see docs:
#   0x0d observed ONLY with 3 args (8-byte instr) in this sample;
#   0x09 param==0 and 0x0d param==0 carry u32 payloads ((0,0)->0.0/8.0 etc).
ARITY = {0x00: 0, 0x05: 1, 0x08: 2, 0x09: 2, 0x0d: 3, 0x0a: 2}


class Sbsasm:
    def __init__(self, data: bytes, xml_map: dict | None = None):
        self.d = data
        self.xml = xml_map or {}
        self.hdr = {}
        self.records = []        # (off, kind, detail)
        self.instrs = []         # (off, op, param, args, note)
        self.unknowns = collections.Counter()   # (off, hexbytes) unknown starts
        self.op_hist = collections.Counter()
        self.uid_hits = collections.Counter()
        self.cover = [0] * len(data)            # classification per byte

    # ---- header -----------------------------------------------------------
    def parse_header(self):
        d = self.d
        if d[:4] != b"SBAM":
            raise SystemExit("not an SBAM file (magic=%r)" % d[:4])
        u32 = lambda o: struct.unpack_from("<I", d, o)[0]
        self.hdr = {
            "version": struct.unpack_from("<HH", d, 4),        # (0, 2) observed
            "cookerbuild": u32(8),                              # == XML cookerbuild
            "asmuid": u32(12),                                  # == XML asmuid
            "filesize": u32(16),
            "tail_size": u32(20),                               # observed 28
            "tail_offset": u32(28),                             # filesize - tail_size
            "offset_table_count": u32(0x140),                   # observed 0x40 == 64
            "offset_table": [u32(0x38 + 4 * i) for i in range(65)],
        }

    # ---- uid extraction (XML cross reference) ------------------------------
    def extract_uids(self):
        """Return sorted set of plausible u32 LE uids in file + their offsets."""
        hits = collections.defaultdict(list)
        for off in range(0, len(self.d) - 4):
            u = struct.unpack_from("<I", self.d, off)[0]
            # observed uid ranges from this corpus: 0x0A12xxxx, 0x5D6xxxxx,
            # 0x2076/0x2081/0x2082/0x2088/0x208F...  crude but effective filter
            hi = u >> 24
            if hi in (0x0A, 0x20, 0x5D, 0x97, 0x4D) or u > 100_000_000 and u < 4_294_967_295:
                # only keep ones the XML knows, or that look like a family
                if u in self.xml or hi in (0x0A, 0x20, 0x5D):
                    hits[u].append(off)
        return hits

    # ---- record markers ----------------------------------------------------
    def scan_records(self):
        """
        Function-body records: sibling bodies differ ONLY in one byte
        (node id 0x88..0xcc).  LE u32 markers observed:
            0x0398aa04 (begin, flav=0x9803)  ...  0x0018aa02 (end, flav=0x1802)
            0x03988804 / 0x03198804 variants too.
        Treating any u32 matching 0x03??XX04 / 0x00??XX02 family as anchor.
        """
        d = self.d
        for off in range(0x140, len(d) - 4):
            w = struct.unpack_from("<I", d, off)[0]
            b = d[off:off + 4]
            if b[3] in (0x03, 0x00) and b[2] in (0x98, 0x19, 0x18) and b[0] < 0xE0:
                # looks like <flavour 2B><nodeid u8><u8 03/04/02>
                self.records.append(
                    (off, "ANCHOR", b.hex(" "), w))
                RECORD_U32.add(w)

    # ---- block detection ---------------------------------------------------
    def find_blocks(self):
        """Hypothesis: opcode runs start at 2-byte head 0x002a / 0x0002 (op 0,
        param 0x2a/2).  Find all such starts, decode forward until unknown."""
        starts = sorted(set(self._find_head(0x2a) + self._find_head(0x02)))
        blocks = []
        for s in starts:
            if s <= 0x140 or s >= len(self.d) - 8:
                continue
            blocks.append(s)
        return blocks

    def _find_head(self, param):
        pat = struct.pack("<H", param)
        out, i = [], 0
        while True:
            i = self.d.find(pat, i)
            if i < 0:
                break
            out.append(i)
            i += 1
        return out

    # ---- instruction decode -------------------------------------------------
    def decode(self):
        d = self.d
        E = len(d)
        for pos in self.find_blocks():
            cur = pos
            while cur < E - 2:
                head = struct.unpack_from("<H", d, cur)[0]
                op, param = head >> 8, head & 0xFF
                if op not in ARITY:
                    # resync: consume 4 bytes if a record-anchor u32 sits here
                    w = struct.unpack_from("<I", d, cur)[0]
                    b = d[cur:cur + 4]
                    if b[2] in (0x98, 0x19, 0x18) and b[3] in (0x03, 0x00) and b[0] < 0xE0:
                        self.instrs.append((cur, 0x100, b[0], (), "ONEANCH"))
                        cur += 4
                        continue
                    self.unknowns[(cur, d[cur:cur + 6].hex(" "))] += 1
                    break                           # resync at next block
                n = ARITY[op]
                if op in (0x09, 0x0d) and param == 0:
                    # param-0 variants carry a u32 payload field ((0,0)->f32)
                    note_f32 = True
                else:
                    note_f32 = False
                end = cur + 2 + 2 * n
                if end + 2 > E:
                    break
                args = struct.unpack_from("<%dH" % n, d, cur + 2)
                note = ""
                if note_f32:
                    if n >= 2:
                        f = struct.unpack_from("<f", d, cur + 2)[0]
                        note = "f32=%r" % (f,)
                elif op == 0x09 and param == 2:
                    uid = struct.unpack_from("<I", d, cur + 2)[0]
                    label = self.xml.get(uid, "?")
                    note = "uid=0x%08x %s" % (uid, label)
                    self.uid_hits[uid] += 1
                elif n == 2 and op == 0x09 and args[1] in F16:
                    note = "f16=%s" % F16[args[1]]
                elif op == 0x0a and param == 2:
                    uid = struct.unpack_from("<I", d, cur + 2)[0]
                    note = "uid=0x%08x %s" % (uid, self.xml.get(uid, "?"))
                    self.uid_hits[uid] += 1
                elif self.xml and len(args) >= 2:
                    # any 4-byte operand window that matches a known uid LE
                    if end and cur + 6 <= len(d):
                        u = struct.unpack_from("<I", d, cur + 2)[0]
                        if u in self.xml:
                            note = "arg-window uid=0x%08x %s" % (
                                u, self.xml.get(u, "?"))
                            self.uid_hits[u] += 1
                self.instrs.append((cur, op, param, args, note))
                self.op_hist[op] += 1
                self.cover[cur:end] = [1] * (end - cur)
                cur = end

    # ---- output -------------------------------------------------------------
    def disasm(self, verbose=False):
        h = self.hdr
        L = []
        A = L.append
        A("=" * 78)
        A("SBAM HEADER (confirmed fields, u32 LE unless noted)")
        A("  magic        : SBAM")
        A("  version      : u16 pair %s" % (h["version"],))
        A("  cookerbuild  : %d" % h["cookerbuild"])
        A("  asmuid       : 0x%08x (%d)" % (h["asmuid"], h["asmuid"]))
        A("  filesize     : %d" % h["filesize"])
        A("  tail.mutable : offset+size = %#x / %d" %
          (h["tail_offset"], h["tail_size"]))
        A("  offset table : count=u32@0x140 = %s ; 65 u32 LE values at 0x38..0x138" %
          h["offset_table_count"])
        A("")
        A("RECORD ANCHORS (function boundary candidates, u32 LE 0x03/0x98/xx):%d" %
          len(self.records))
        if verbose:
            for off, kind, hx, w in self.records[:80]:
                A("  @0x%06x %s %s" % (off, kind, hx))
        A("")
        A("DISASSEMBLY (hypothesized: head [param|op] u16 LE + N u16 args)")
        for (off, op, param, args, note) in self.instrs:
            pretty = []
            for a in args:
                if a in F16 and op == 0x09:
                    pretty.append("%s(/*f16*/ 0x%04x)" % (F16[a], a))
                else:
                    pretty.append("0x%04x" % a)
            A("  @0x%06x op=%#04x p=%#04x seq=0x%06x %s %s" %
              (off, op, param, head_of(self.d, off),
               " ".join(pretty).ljust(26), note))
        skipped = sum(len(d) - 0 for _ in ())  # placeholder
        lo, hi = 0x18C, 0x22E0
        span = hi - lo
        cov = sum(self.cover[lo:hi])
        A("")
        A("STREAM COVERAGE: %d/%d bytes (%.1f%%) of program area [0x18c..0x22e0] decoded"
          % (cov, span, 100.0 * cov / span))
        A("")
        A("")
        A("OP HISTOGRAM (op byte):")
        for op, c in self.op_hist.most_common():
            A("  op=0x%02x  n=%d" % (op, c))
        A("")
        A("UNKNOWN SITES (resync points): %d" % len(self.unknowns))
        for (off, hx), c in self.unknowns.most_common(20):
            A("  @0x%06x x%d bytes=%s" % (off, c, hx))
        A("")
        if self.uid_hits:
            A("PARAMETER UID REFERENCES (op 0x09/0x0a, param 2):")
            for uid, c in self.uid_hits.most_common():
                A("  0x%08x x%d -> %s" % (uid, c, self.xml.get(uid, "<no xml match>")))
        return "\n".join(L)


def head_of(d, off):
    return struct.unpack_from("<H", d, off)[0]


def parse_xml_uids(path):
    """Crude: pull uid=\"<num>\" identifier=\"<string>\" pairs out of an
    sbsdescription XML."""
    import re
    txt = open(path, "r", encoding="utf8", errors="replace").read()
    m = {}
    for mo in re.finditer(
            r'uid="(\d+)"\s+identifier="([^"]+)"', txt):
        m[int(mo.group(1))] = mo.group(2)
    # attributes can come in either order; second pass
    for mo in re.finditer(
            r'identifier="([^"]+)"[^>]*?uid="(\d+)"', txt):
        m.setdefault(int(mo.group(2)), mo.group(1))
    return m


# ----------------------------------------------------------------------------
def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("file")
    ap.add_argument("--xml", help="companion sbsdescription .xml")
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--save", help="also write listing to file")
    args = ap.parse_args()

    data = open(args.file, "rb").read()
    xml_map = {}
    xmlpath = args.xml
    if not xmlpath and args.file.lower().endswith(".sbsasm"):
        guess = args.file[:-len(".sbsasm")] + ".xml"
        if os.path.exists(guess):
            xmlpath = guess
    if xmlpath and os.path.exists(xmlpath):
        xml_map = parse_xml_uids(xmlpath)
        print("xml uid map: %d names from %s" % (len(xml_map), xmlpath))

    fa = Sbsasm(data, xml_map)
    fa.parse_header()
    fa.scan_records()
    fa.decode()
    out = fa.disasm(verbose=args.verbose)
    print(out)
    if args.save:
        open(args.save, "w").write(out)


if __name__ == "__main__":
    main()
