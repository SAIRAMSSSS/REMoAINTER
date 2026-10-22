# Substance Compiled assembly (.sbsasm / "SBAM") — format notes

Reverse-engineered by inspecting one sample: `mg_dust.sbsasm` (9060 bytes,
dated 2015-08-12, cookerbuild 16082), extracted from the `.sbsar` inside
`"Aluminium Anodized Red.spsm"`, cross-referenced against its companion
sbsdescription XML. **No Adobe source code was used — this is container-level
and statistical inference only (legal note: independent re-implementation
research; do not copy Adobe code or assets).**

Confidence key: **[confirmed]** (multiple independent cross-checks with the
XML), **[observed]** (byte evidence, single file), **[hypothesis]** (model that
parses but semantics unknown).

---

## 1. Container header [confirmed]

```
offset  size  field
0x00    4     magic "SBAM"
0x04    2+2   u16,u16  (0, 2)                -- version? file has bytes 00 00 02 00
0x08    4     u32      cookerbuild = 16082    -- matches XML attr cookerbuild
0x0C    4     u32      asmuid     = 0x97CBA38E -- matches XML attr asmuid="2546705294"
0x10    4     u32      equals file size (9060)
0x14    4     u32      28                     -- size of trailing descriptor tail
0x18    4     u32      0
0x1C    4     u32      9032 (= filesize - 28) -- offset of trailing section
0x20    4     u32      0x00010002
0x24    4     u32      0
0x28    4     u32      1                      -- graph count (XML: graphs count="1")?
0x2C    4     u32      0x22D0                 -- offset of trailing "graph default"
                                                block (f32 0.5,0.5,0.5 = slider
                                                defaults in XML) [observed]
0x30    4     u32      2
0x34    4     u32      0x110 -- start of first entry (see below) [observed]
0x38   0x104* u32[65]  ascending offsets 0x110 .. 0x2250 (see offset table)
```

All integers **little-endian** [confirmed — asmuid/cookerbuild/uids read
back exactly].

### Offset table @0x38..0x13C (65 u32 entries)
Ascending offsets into the file, ending < 0x22E0 (end of code area).
Interpretation [hypothesis]: start offsets of the 64+1 "records/sections"
(whole-file chunk directory). Last entry 0x2250, followed at +0x13C by
u32 `0x010000C0` (terminator? not a plausible file offset).

### Secondary table region @0x140..0x18C
```
0x140  u32  0x40 (64)                     -- count? (65-64=+1 discrepancy [observed])
0x144  u32  0x03984408                     -- record marker (see §3)
0x148.. u32 4                               -- ?
0x14C,0x150  u32 0x124, 0x164              -- offsets (< current pos) or rel-base?
0x14C+   rows of 8 bytes: {u16 8, u16 2, u32 ptr}  (ptr: 0x12C..0x4C)
one 12-byte row {u16 0x48, u16 0x10, u32 0x154, u32 0x158}
```
Layout unconfirmed; values are plausible point-of-use tables ("vessel"/register
plan) but no decode achieved yet. **NEXT-STEP candidate.**

---

## 2. Program area 0x18C..0x22E0

Sequential instruction stream, no padding. Everything is **u16 LE oriented**.

Instruction model [observed, self-consistent for ~1600 instrs, 75% byte
coverage of the program area]:

```
instr :=  head:u16 LE  +  N u16 LE operands
head  :=  [b0][b1] = [PARAM][OP]
```

i.e. the opcode is the **second** byte, the byte register/sel is first.

Opcodes observed with op byte / default operand count / evidence:

| op  | params seen               | operands | hypothesis name |
|-----|---------------------------|----------|-----------------|
|0x00 | 0x00..0x30, 0x01, 0x02, 0x2a, 0x0f | 0 | boundary / label / nop [observed] |
|0x05 | 0x16,0x24,0x2b,0x32,0x41,0x51 | 1 | unary-ish op |
|0x08 | 0x5d,0x5f,0x61            | 2        | binary op |
|0x09 | many (0x09,0x0d,0x10,0x12,0x13,0x14,0x15,0x30,0x31,0x4d,0xcd,0x02,0x00) | 2 | main ALU/select family |
|0x0a | 0x02,0x12,0x42            | 2        | uid-reference op |
|0x0d | 0x00,0x09,0x2f            | 3        | select/combine (ternary-ish) |

Confirmed numeric-constant encodings:
* `00 09 00 00 80 3f` = op 0x09, param 0, operand bytes `00 00 80 3f` =
  **f32LE 1.0**. Params 0 variants of op 0x09 / 0x0d carry a f32:
  1.0, 0.0, 0.25 (bytes `01 80 3e` as LE u16 `3e80`), 0.5, and 8.0 (bytes
  `10 41` = LE u16 `0x4110`; f32 8.0 would be `00 00 20 41`, so the 8.0 byte
  may be a u32 payload, not a guaranteed f32). **[observed]**
* The pair `(0x0000, <half>)` as two u16 raw args co-occurs; both readings
  size fit.
* op 0x0d than 3 raw u16 args, e.g. `00 0d | 00 00 | 00 00 | 00 3f`
  (args 0,0, and "0.5") [observed].

### Parameter-UID immediate [confirmed]
`02 09 <uid:u32LE>` = head `02 09`, followed by 4 bytes that, read as LE u32,
equal XML input uids:
```
from mg_dust.xml  ->  bytes in sbsasm (LE u32):
$randomseed        169012851  -> 73 ee 12 0a                  (18 in-file refs)
$outputsize        1566636065 -> 21 f8 60 5d                  (32 in-file refs)
invert             545344542  -> 1e 4c 81 20
Level              545355339  -> 4b 76 81 20
Contrast           545355341  -> 4d 76 81 20
Occlusion_Amount   545355343  -> 4f 76 81 20
Noise_Opacity      545355345  -> 51 76 81 20
world_space_normals545355439 -> af 76 81 20
ambient_occlusion  545429278  -> 1e 97 82 20
output(OUT)        545426227  -> 33 8b 82 20
```
NOTE the "Level/Contrast/…" family share prefix `0x2081 76` — uids are
sequential; the compiler emits them verbatim in the stream **[confirmed]**.
The uid-bearing instruction varies: params 2 (0x09/0x02 families), and
`42 0a`-headed byte pair followed by u32 — the uid offset bytes are always
LE [confirmed endianness].

### Boundary / record markers [observed]
* `01 00 42 0a <uid4>` — 8-byte record; seen bounding function bodies with
  `$outputsize`/`invert` uids. Possibly "set result slot from user param".
* `04 xx 98 yy` / `02 xx 18 yy` — u32 pairs bracketing function bodies:
  same node offsets (0x88,0x99,0xaa,0xbb,0xcc) always paired; the second
  dword `18 yy 00 00` encodes `<u32 sizes / defaults list>` after it:
  e.g. `02 99 18 00 | 10 00 00 00 | 20 00 00 00 | 21 00 00 00 | 00 00 00 3f`
  = u32 trio (0x10,0x20,0x21) + f32 0.5 (matches "Contrast" default
  0.5 in XML!) [observed].
* `04 aa 98 03 5f 00 00 04 <u32> <u32> <f32 a> <f32 b> <f32 c> <f32 d> <u32>`
  = begin-record with an f32 quad (2.0, 0.0, 0.0, 2.0) and a trailing u32
  pointing back to the function body start [hypothesis].

### Repeated function body shape [observed]
One full per-float body structure observed verbatim and duplicated ~7 times
for different node ids / local count fields [observed]; stack slots are
addressed by small u16s 1..0x41:
```
2a00 0009 0000803f          (label? + const 1.0)
3205 0000                   op05 32, arg 0
4105 0100                   op05 41, arg 1
1009 0200 0000              op09 10, args 2,0
0009 <f32>                  const push
1509 x2                     op09 15 (two-arg) ×...
2405 n  /  2b05 n           op05 store-ish
1209 a b / 1409 a b / 1309 a b   op09 2-arg ALU variants
000d 0 0 <half>             op0d "clamp/select with threshold"?
5f08 a b  / 61 08 a b       op08 binary
090d a b c                  op0d (param 0x09) 3-arg
4d09 a b / cd09 a b         terminal op09 (param 0x4d/0xcd high bits: condition?)
0000                        op00 end-of-body
```

### Trailing descriptor tables (0x22E0..end)
1. Graph default block [confirmed]: `01 00 42 0a 21 f8 60 5d` ($outputsize)
   then f32 0.5, f32 0.5, f32 0.5, 0, 8 — the three sliders' 0.5 defaults and
   `$outputsize = 8,8` from XML ✓✓.
2. Final u32 table 0x2348..end (28 bytes) = output/parameter uid list
   [observed; exact layout not yet decoded].

---

## 3. What is NOT understood (honest limits)
* Exact op semantics per param value (0x0532 vs 0x0541 etc.).
* The 65-entry offset table at 0x38 and the 0x140..0x18C compact tables.
* `00 0e ...` marker records and the 16-byte "exports" after `02 0a` uid ops.
* Op 0x0a param 0x42/0x12/0x02 differences.
* Only ONE distinct .sbsasm was available (all .spsm files in the test dir
  contained the same mg_dust .sbsar; the injected HDF body is
  ~1.1 MB of RIFF/WebP + other preview data). All byte-level conclusions are
  from ONE file, though its ~15 near-identical internal function bodies give
  mitigation via self-diffing.

---

## 4. NEXT STEPS (highest value first)
1. **Acquire more distinct .sbsasm files.** Extract a different Substance
   (.sbsar from a full material, not a smart material) and diff *function
   body pairs* per node id 0x88..0xcc: bodies should differ only in NaN-free
   ways (local count / node id byte). That isolates op semantics for
   op 0x05/0x08/0x0d/0x09-param variants. (The single margin we could not
   resolve: what op 0x5f08 vs op 0x6108 differ by.)
2. Try inferring operand graphs: treat u16 args as SSA vreg indices →
   topological order should reach a value whose op chain ⊢ matches
   `Level`/`Contrast` slider math (lerp/clamp) — cross-check by string.
   Then decode the mysterious `2b05`/`2405` "store slot" ops.
3. Decode the header's 65-entry offset table as an *offsets-into-code* list;
   check the 0x18C block start hypothesis by whether each listed offset
   starts with the archive structure (0100 420a ...).
4. The pre-table at 0x140..0x18C has entries {u16 8, u16 2, u32 ptr} and one
   {u16 0x48, u16 0x10, u32 0x154, u32 0x158}: probe whether ptr values point
   *relative* (add 0x100? add 0x18?) to table rows elsewhere.

Tool: `tools/sbsasm_disasm.py` implements this walk; run
`python3 tools/sbsasm_disasm.py x.sbsasm --xml x.xml --save out.txt`.
Example result for mg_dust.sbsasm: 75.1% byte coverage, 6 op classes,
54 unknown-run sites all at record boundaries — see its output.
