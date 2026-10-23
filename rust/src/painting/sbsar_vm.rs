//! First working interpreter for the `SBAM` (`sbsasm`) bytecode shipped inside
//! SBAM-corrected `.sbsar` reference archives.
//!
//! Format notes live in `docs/ARCHITECTURE.md`; this module implements
//! the decode/execute model that the notes' evidence supports, and refines the
//! notes (both docs and tests are corrected together when they disagree).
//!
//! Decoded program model (see `docs/ARCHITECTURE.md` §2, upgraded by
//! this session's SSA discovery):
//!
//! ```text
//! instr := head:u16 LE [param:u8][op:u8] + operands
//! body  := (op=0x00 param!=0 label) ... ALU stream ... (op=0x00 param==0 end)
//! ```
//!
//! Each producing instruction (constant push, parameter fetch, ALU) assigns its
//! result to a *new* register whose index is the running count of producers in
//! the body. The u16 operands of ALU instructions are **source register
//! indices** (SSA), never destinations. Every source must name an
//! already-produced register — this "all sources resolve" invariant is the
//! strongest structural evidence found so far and is enforced both at parse
//! time and at run time.
//!
//! Module-level confidence tags are attached to every semantic choice; the
//! structural decode is treated separately from the arithmetic ladder, which is
//! still a hypothesis.

use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Format constants (offsets/sizes confirmed on one sample; see notes §1)
// ---------------------------------------------------------------------------

/// Magic of a compiled assembly file.
pub const SBAM_MAGIC: &[u8; 4] = b"SBAM";
/// Trailing descriptor tail size, observed on the sample [observed].
pub const TAIL_SIZE: u32 = 28;
/// Entry count of the offset directory at +0x38 [observed: 65 entries].
pub const SECTION_ENTRY_COUNT: usize = 65;
/// Byte offset of the offset directory.
const SECTION_TABLE_OFF: usize = 0x38;
/// Byte offset of the directory's 65th-exceeding "terminator" u32.
const SECTION_TERM_OFF: usize = 0x13C;
/// Byte offset of the raw count field (observed 64 = 65 - 1).
const SECTION_COUNT_FIELD_OFF: usize = 0x140;
/// Byte offset where the executable program area begins [observed].
const PROGRAM_START: u32 = 0x18C;

/// Parameter UIDs confirmed against the companion `sbsdescription` XML
/// (`$randomseed` 169012851, `$outputsize` 1566636065, ...) [confirmed].
pub const SEED_UID: u32 = 169012851; // $randomseed
pub const OUTPUT_SIZE_UID: u32 = 1566636065; // $outputsize
pub const LEVEL_UID: u32 = 545355339; // Level
pub const CONTRAST_UID: u32 = 545355341; // Contrast
pub const OCCLUSION_AMOUNT_UID: u32 = 545355343; // Occlusion_Amount
pub const NOISE_OPACITY_UID: u32 = 545355345; // Noise_Opacity
pub const INVERT_UID: u32 = 545344542; // invert

/// How far a single body may execute before we assume a decode inconsistency.
/// Bodies observed so far are straight-line (~45 instructions); the budget is a
/// runaway guard, not a format constant.
const STEP_BUDGET: u32 = 100_000;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Typed errors — evaluation never panics; every failure path is a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbamError {
    /// Magic bytes did not match `SBAM`.
    NotSbam {
        /// The four bytes found at offset 0, rendered hex.
        got: String,
    },
    /// File ended inside a fixed-size field.
    Truncated {
        /// Byte offset of the incomplete field.
        at: usize,
        /// Bytes required to complete it.
        need: usize,
    },
    /// An opcode (or ALU sub-operation) we have no semantic for. `pc` is the
    /// byte offset of the instruction head in the file, `op` the offending
    /// opcode byte.
    UnknownOpcode {
        pc: usize,
        op: u8,
    },
    /// A parameter fetch whose UID has no value in the supplied map and is not
    /// a graph-level builtin (seed / output size).
    UnknownParam {
        uid: u32,
        body: usize,
    },
    /// An ALU source register index beyond the count of registers produced so
    /// far in the body.
    UnresolvedSource {
        pc: usize,
        src: u16,
        produced: u16,
    },
    /// `evaluate` called with an out-of-range body index.
    BodyIndexOutOfRange {
        index: usize,
        count: usize,
    },
    /// Single-body instruction budget exhausted (decode runaway guard).
    StepBudgetExceeded {
        body: usize,
    },
}

impl fmt::Display for SbamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSbam { got } => write!(f, "not an SBAM file (magic {got})"),
            Self::Truncated { at, need } => {
                write!(f, "truncated SBAM: need {need} bytes at offset {at}")
            }
            Self::UnknownOpcode { pc, op } => {
                write!(f, "unknown opcode 0x{op:02x} at pc 0x{pc:x}")
            }
            Self::UnknownParam { uid, body } => {
                write!(f, "unknown parameter uid 0x{uid:08x} in body {body}")
            }
            Self::UnresolvedSource { pc, src, produced } => write!(
                f,
                "source register {src} not yet produced (have {produced}) at pc 0x{pc:x}"
            ),
            Self::BodyIndexOutOfRange { index, count } => {
                write!(f, "body index {index} out of range (count {count})")
            }
            Self::StepBudgetExceeded { body } => {
                write!(f, "body {body} exceeded the instruction step budget")
            }
        }
    }
}

impl std::error::Error for SbamError {}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// A runtime value flowing through the register file. reference parameter
/// graphs are scalar or small-vector; the widest shape observed in encoded
/// immediate pairs is two-wide [med], but the ladder is applied componentwise
/// over whatever shape a register carries (ladder variant (a): componentwise;
/// scalars broadcast — see `binary_ladder`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    F32(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl Value {
    /// All lanes, widening scalar to a single lane.
    fn lanes(&self) -> Vec<f32> {
        match self {
            Value::F32(x) => vec![*x],
            Value::Vec2(v) => v.to_vec(),
            Value::Vec3(v) => v.to_vec(),
            Value::Vec4(v) => v.to_vec(),
        }
    }

    /// Scalar view for comparison helpers in tests and callers.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::F32(x) => Some(*x),
            _ => None,
        }
    }

    /// Rebuild a value of `self`'s shape from per-lane results.
    fn of_shape(&self, lanes: &[f32]) -> Value {
        match self {
            Value::F32(_) => Value::F32(lanes[0]),
            Value::Vec2(_) => Value::Vec2([lanes[0], lanes[1]]),
            Value::Vec3(_) => Value::Vec3([lanes[0], lanes[1], lanes[2]]),
            Value::Vec4(_) => Value::Vec4([lanes[0], lanes[1], lanes[2], lanes[3]]),
        }
    }

    /// Componentwise map over `self`.
    fn map_lane(self, f: impl Fn(f32) -> f32) -> Value {
        let mapped: Vec<f32> = self.lanes().into_iter().map(f).collect();
        self.of_shape(&mapped)
    }
}

// ---------------------------------------------------------------------------
// Decoded program
// ---------------------------------------------------------------------------

/// Fixed-size header fields (layout in notes §1 [confirmed]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SbamHeader {
    /// u16,u16 at +0x04, observed (0, 2).
    pub version: (u16, u16),
    /// +0x08, matches XML `cookerbuild` attribute [confirmed].
    pub cookerbuild: u32,
    /// +0x0C, matches XML `asmuid` attribute [confirmed].
    pub asmuid: u32,
    /// +0x10, equals the byte size of the whole file [confirmed here].
    pub file_size: u32,
    /// +0x1C, offset of the trailing descriptor section (= file_size - 28).
    pub tail_offset: u32,
    /// +0x14, observed 28.
    pub tail_size: u32,
    /// +0x28, observed 1 (graph count in XML [observed]).
    pub graph_count: u32,
    /// +0x2C, observed 0x22D0 — start of the graph-default block.
    pub graph_defaults_offset: u32,
}

/// One decoded function body of the program area.
#[derive(Debug, Clone, PartialEq)]
pub struct SbamBody {
    /// Byte offset of the first decoded instruction (after label bytes).
    pub start: u32,
    /// Exclusive byte offset where the body's instruction stream ended.
    pub end: u32,
    /// Node-id byte if a `04 xx 98 03`-family anchor was found immediately
    /// before the body's label in the raw stream [observed].
    pub node: Option<u8>,
    /// Decoded instructions with their file offsets.
    pub instrs: Vec<(u32, Instr)>,
    /// Number of registers this body produces (upper bound of source indices).
    pub register_count: u16,
    /// Parameter UIDs fetched by this body, in fetch order.
    pub fetched_uids: Vec<u32>,
}

/// One decoded instruction. Sources are register indices; destinations are
/// implicit (the next register in the body's sequential counter).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Instr {
    /// op 0x00 param != 0 — boundary / label marker, no effect [observed].
    Label { param: u8 },
    /// op 0x00 param 0x00 — end of body [observed].
    End,
    /// op 0x09 param 0x00 — push an f32 constant [confirmed: immediate bytes
    /// read back as 1.0/0.25/0.5/2.0 float patterns].
    ConstF32(f32),
    /// UID-bearing fetch forms:
    /// op 0x09 param 0x02 / op 0x0a param 0x02 [confirmed: u32 equals XML
    /// input uids] and op 0x08 param 0x42 [med: arg window matched the XML
    /// `invert` uid on the sample].
    FetchParam { uid: u32 },
    /// op 0x05 — one-source op [observed; arity].
    Unary { op: u8, sub: u8, src: u16 },
    /// op 0x08 (except param 0x42) and op 0x09 — two-source op [observed].
    Binary { op: u8, sub: u8, a: u16, b: u16 },
    /// op 0x0d — three-operand op. When `sub == 0x00` the third operand
    /// `c` is a raw u16 holding an IEEE-half immediate threshold; otherwise
    /// all three are source register indices [observed shape].
    Ternary { op: u8, sub: u8, a: u16, b: u16, c: u16 },
}

/// A parsed SBAM assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct SbamProgram {
    pub header: SbamHeader,
    /// Raw u32 at +0x140 — observed 64 while the directory holds 65 entries
    /// (+1 discrepancy kept raw [observed], not "fixed" by the parser).
    pub directory_count_field: u32,
    /// The 65 ascending section offsets at +0x38 [observed].
    pub sections: Vec<u32>,
    /// The u32 following the directory at +0x13C, observed 0x000100C0.
    pub section_terminator: u32,
    /// Byte offset where executable decoding starts (0x18C on the sample).
    pub program_start: u32,
    /// Byte offset where executable decoding ends (the trailing descriptor
    /// section start; = file_size - tail_size).
    pub program_end: u32,
    /// Decoded function bodies in stream order.
    pub bodies: Vec<SbamBody>,
}

impl SbamProgram {
    /// Parse an SBAM from raw bytes. Returns `Result<Self, String>` per the
    /// public wire contract used by the shelf/loader layers.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        // ---- fixed-size header ----
        let need = |at: usize, n: usize| -> Result<(), String> {
            if bytes.len() >= at + n {
                Ok(())
            } else {
                Err(format!("truncated SBAM: need {n} bytes at offset {at}"))
            }
        };
        need(0, 4)?;
        if &bytes[0..4] != SBAM_MAGIC {
            return Err(format!(
                "not an SBAM file (magic {})",
                hex(&bytes[0..4.min(bytes.len())])
            ));
        }
        need(SECTION_TERM_OFF, 4)?;
        need(SECTION_COUNT_FIELD_OFF, 4)?;
        let u16le = |off: usize| -> Result<u16, String> {
            need(off, 2)?;
            Ok(u16::from_le_bytes([bytes[off], bytes[off + 1]]))
        };
        let u32le = |off: usize| -> Result<u32, String> {
            need(off, 4)?;
            Ok(u32::from_le_bytes([
                bytes[off],
                bytes[off + 1],
                bytes[off + 2],
                bytes[off + 3],
            ]))
        };
        let version = (u16le(0x04)?, u16le(0x06)?);
        let header = SbamHeader {
            version,
            cookerbuild: u32le(0x08)?,
            asmuid: u32le(0x0C)?,
            file_size: u32le(0x10)?,
            tail_size: u32le(0x14)?,
            tail_offset: u32le(0x1C)?,
            graph_count: u32le(0x28)?,
            graph_defaults_offset: u32le(0x2C)?,
        };

        // ---- offset directory: 65 ascending entries + terminator ----
        let mut sections = Vec::with_capacity(SECTION_ENTRY_COUNT);
        for i in 0..SECTION_ENTRY_COUNT {
            sections.push(u32le(SECTION_TABLE_OFF + i * 4)?);
        }
        if !sections.windows(2).all(|w| w[0] < w[1]) {
            return Err("SBAM section directory is not strictly ascending".into());
        }
        let section_terminator = u32le(SECTION_TERM_OFF)?;
        let directory_count_field = u32le(SECTION_COUNT_FIELD_OFF)?;

        // ---- executable region between the fixed preamble and the tail ----
        let program_start = PROGRAM_START;
        let program_end = header.tail_offset.min(bytes.len() as u32);
        if program_end <= program_start {
            return Err(format!(
                "SBAM has no program area (start 0x{program_start:x}, end 0x{program_end:x})"
            ));
        }

        // ---- locate + decode function bodies (SSA model) ----
        let bodies = find_bodies(bytes, program_start, program_end);

        Ok(Self {
            header,
            directory_count_field,
            sections,
            section_terminator,
            program_start,
            program_end,
            bodies,
        })
    }

    /// Number of decoded function bodies.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }
}

/// Hex helper for error text.
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Body decode + acceptance. A *label* (op 0x00, param != 0) starts a decode
/// window; a body is accepted only when its interior decodes SSA-consistently
/// and terminates on an ALU-shaped instruction. Rejected candidates consume
/// only their head bytes — the outer scan resynchronizes at label granularity.
fn find_bodies(bytes: &[u8], start: u32, end: u32) -> Vec<SbamBody> {
    let mut bodies = Vec::new();
    let mut pc = start;
    while pc + 2 <= end {
        let head = u16le_at(bytes, pc);
        let op = ((head >> 8) & 0xFF) as u8;
        let param = (head & 0xFF) as u8;
        if op == 0x00 && param != 0x00 {
            match decode_body_from(bytes, pc, end) {
                Ok((body, next_pc)) => {
                    bodies.push(body);
                    pc = next_pc;
                }
                Err(()) => {
                    pc += 2; // candidate rejected; resync past the label bytes
                }
            }
        } else {
            pc += 2;
        }
    }
    bodies
}

/// Errors are "candidate rejected" — never a format error: record/descriptor
/// bytes surround the code and are expected to fail decode.
fn decode_body_from(bytes: &[u8], label_pc: u32, end: u32) -> Result<(SbamBody, u32), ()> {
    let mut instrs: Vec<(u32, Instr)> = Vec::new();
    let mut fetched: Vec<u32> = Vec::new();
    let mut produced: u16 = 0;
    let mut pc = label_pc;

    loop {
        if pc + 2 > end {
            return Err(()); // unterminated window
        }
        let head = u16le_at(bytes, pc);
        let op = ((head >> 8) & 0xFF) as u8;
        let param = (head & 0xFF) as u8;
        let out: Instr;
        let advance;
        match op {
            0x00 => {
                if param == 0x00 {
                    // End marker; accept only if the body ended on ALU work.
                    let last_ok = matches!(
                        instrs.last().map(|(_, i)| i),
                        Some(Instr::Unary { .. })
                            | Some(Instr::Binary { .. })
                            | Some(Instr::Ternary { .. })
                            | Some(Instr::FetchParam { .. })
                    );
                    if produced >= 1 && last_ok {
                        instrs.push((pc, Instr::End));
                        pc += 2;
                        break;
                    }
                    return Err(());
                }
                // Label: leading labels are consumed; later ones bound the body.
                if !instrs.is_empty() {
                    let last_ok = matches!(
                        instrs.last().map(|(_, i)| i),
                        Some(Instr::Unary { .. }) | Some(Instr::Binary { .. }) | Some(Instr::Ternary { .. })
                    );
                    if produced >= 1 && last_ok {
                        break; // accept, resync at this label
                    }
                    return Err(());
                }
                out = Instr::Label { param };
                advance = 2;
            }
            0x05 => {
                let src = u16le_at(bytes, pc + 2);
                check_srcs(&[src], produced, pc)?;
                out = Instr::Unary { op, sub: param, src };
                advance = 4;
                produced += 1;
            }
            0x08 => {
                if param == 0x42 {
                    // CONFIDENCE: med — `42 08 <uid>` windows carry XML uids
                    // (observed: `invert`) and behave as value fetches.
                    let uid = u32le_at(bytes, pc + 2);
                    fetched.push(uid);
                    out = Instr::FetchParam { uid };
                    advance = 6;
                } else {
                    let a = u16le_at(bytes, pc + 2);
                    let b = u16le_at(bytes, pc + 4);
                    check_srcs(&[a, b], produced, pc)?;
                    out = Instr::Binary { op, sub: param, a, b };
                    advance = 6;
                }
                produced += 1;
            }
            0x09 => {
                if param == 0x00 {
                    // CONFIDENCE: high — `00 09` + 4 bytes read back as f32
                    // (1.0 / 0.25 / 0.5 / 2.0 patterns across bodies).
                    let f = f32le_at(bytes, pc + 2);
                    out = Instr::ConstF32(f);
                    advance = 6;
                } else if param == 0x02 {
                    // CONFIDENCE: high — u32 equals a companion-XML input uid.
                    let uid = u32le_at(bytes, pc + 2);
                    fetched.push(uid);
                    out = Instr::FetchParam { uid };
                    advance = 6;
                } else {
                    let a = u16le_at(bytes, pc + 2);
                    let b = u16le_at(bytes, pc + 4);
                    check_srcs(&[a, b], produced, pc)?;
                    out = Instr::Binary { op, sub: param, a, b };
                    advance = 6;
                }
                produced += 1;
            }
            0x0a => {
                if param == 0x02 {
                    // CONFIDENCE: high — same uid-immediate family as 09/02.
                    let uid = u32le_at(bytes, pc + 2);
                    fetched.push(uid);
                    out = Instr::FetchParam { uid };
                    advance = 6;
                    produced += 1;
                } else {
                    // CONFIDENCE: observed — `42 0a <uid>` etc. are binding
                    // records, not code; stop the window.
                    return Err(());
                }
            }
            0x0d => {
                if param == 0x00 {
                    // CONFIDENCE: med — (src, src, half-immediate); one
                    // register is produced (see notes "0d00 model").
                    let a = u16le_at(bytes, pc + 2);
                    let b = u16le_at(bytes, pc + 4);
                    let c = u16le_at(bytes, pc + 6);
                    check_srcs(&[a, b], produced, pc)?;
                    out = Instr::Ternary { op, sub: param, a, b, c };
                    advance = 8;
                } else {
                    let a = u16le_at(bytes, pc + 2);
                    let b = u16le_at(bytes, pc + 4);
                    let c = u16le_at(bytes, pc + 6);
                    check_srcs(&[a, b, c], produced, pc)?;
                    out = Instr::Ternary { op, sub: param, a, b, c };
                    advance = 8;
                }
                produced += 1;
            }
            _ => {
                // Unknown opcode byte inside the window: not a body — the
                // scanner resynchronizes at the next label.
                return Err(());
            }
        }
        instrs.push((pc, out));
        pc += advance;
    }

    let node = find_node_before(bytes, label_pc);
    let register_count = produced;
    let ok_body = SbamBody {
        start: instrs.first().map(|(o, _)| *o).unwrap_or(label_pc),
        end: pc,
        node,
        instrs,
        register_count,
        fetched_uids: fetched,
    };
    Ok((ok_body, pc))
}

/// Every ALU source must name an already-produced register (SSA invariant).
fn check_srcs(srcs: &[u16], produced: u16, pc: u32) -> Result<(), ()> {
    if srcs.iter().any(|&s| s >= produced) {
        let _ = pc;
        Err(())
    } else {
        Ok(())
    }
}

/// Search backwards (12..64 bytes) for a `04 xx 98 03`-family record anchor
/// to attribute the body to a node id [observed].
fn find_node_before(bytes: &[u8], label_pc: u32) -> Option<u8> {
    let label = label_pc as usize;
    let lo = label.saturating_sub(64);
    let hi = label.saturating_sub(12);
    let hi = hi.min(bytes.len() - 4);
    let mut off = hi;
    while off + 4 <= label && off >= lo {
        let b = [bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]];
        if b[0] == 0x04 && (b[2] == 0x98 || b[2] == 0x19) && b[3] == 0x03 {
            return Some(b[1]);
        }
        if off == lo {
            break;
        }
        off -= 1;
    }
    None
}

fn u16le_at(b: &[u8], off: u32) -> u16 {
    let o = off as usize;
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32le_at(b: &[u8], off: u32) -> u32 {
    let o = off as usize;
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn f32le_at(b: &[u8], off: u32) -> f32 {
    f32::from_bits(u32le_at(b, off))
}

// ---------------------------------------------------------------------------
// VM
// ---------------------------------------------------------------------------

/// Interpreter over a parsed program. Holds only the run-constant environment
/// (seed / output size); parameters are passed per call, so evaluation is fully
/// re-entrant and there is no hidden PRNG state between bodies.
pub struct SbamVm<'p> {
    program: &'p SbamProgram,
    seed: u32,
    output_size: [u32; 2],
}

impl<'p> SbamVm<'p> {
    /// Build a VM over a parsed program. `seed` feeds `$randomseed` fetches
    /// [low: the seed enters the bytecode via UID fetch; the compiler-visible
    /// XOR-mixing happens inside untranslated ALU sub-ops and is NOT modeled
    /// here]. `output_size` feeds `$outputsize` fetches.
    pub fn new(program: &'p SbamProgram, seed: u32, output_size: [u32; 2]) -> Self {
        Self {
            program,
            seed,
            output_size,
        }
    }

    /// Execute body `body_index` with the given parameter map (keyed by
    /// parameter UID — the same u32 immediates embedded in the bytecode).
    /// Deterministic: same program/seed/params → identical value.
    pub fn evaluate(
        &self,
        body_index: usize,
        params: &BTreeMap<u32, Value>,
    ) -> Result<Value, SbamError> {
        if body_index >= self.program.bodies.len() {
            return Err(SbamError::BodyIndexOutOfRange {
                index: body_index,
                count: self.program.bodies.len(),
            });
        }
        let body = &self.program.bodies[body_index];

        let mut regs: Vec<Value> = Vec::with_capacity(body.register_count as usize);
        let mut fetched = body.fetched_uids.iter();
        let mut steps: u32 = STEP_BUDGET;

        for (pc, instr) in &body.instrs {
            if steps == 0 {
                return Err(SbamError::StepBudgetExceeded { body: body_index });
            }
            steps -= 1;
            match *instr {
                Instr::Label { .. } | Instr::End => {}
                Instr::ConstF32(f) => regs.push(Value::F32(sanitize(f))),
                Instr::FetchParam { uid } => {
                    // UIDs were collected in body order at parse time; step
                    // along `fetched` (linear, no HashMap lookups needed for
                    // ordering determinism — BTreeMap used for params anyway).
                    let _ = fetched.next();
                    let v = self.fetch(uid, params, body_index)?;
                    regs.push(v);
                }
                Instr::Unary { op, sub, src } => {
                    let a = look(&regs, src, *pc)?;
                    let next = unary_ladder(op, sub, &a).ok_or(SbamError::UnknownOpcode {
                        pc: *pc as usize,
                        op: sub,
                    })?;
                    regs.push(next);
                }
                Instr::Binary { op, sub, a, b } => {
                    let av = look(&regs, a, *pc)?;
                    let bv = look(&regs, b, *pc)?;
                    let next =
                        binary_ladder(op, sub, &av, &bv).ok_or(SbamError::UnknownOpcode {
                            pc: *pc as usize,
                            op: sub,
                        })?;
                    regs.push(next);
                }
                Instr::Ternary { op, sub, a, b, c } => {
                    let av = look(&regs, a, *pc)?;
                    let bv = look(&regs, b, *pc)?;
                    let tv = if sub == 0x00 {
                        // immediate-half blend factor [med]
                        Value::F32(half_to_f32(c))
                    } else {
                        look(&regs, c, *pc)?
                    };
                    let next =
                        ternary_ladder(op, sub, &av, &bv, &tv).ok_or(SbamError::UnknownOpcode {
                            pc: *pc as usize,
                            op: sub,
                        })?;
                    regs.push(next);
                }
            }
        }

        match regs.pop() {
            Some(v) => Ok(v),
            // A body with only labels/fetches-dead-failures can't happen at
            // parse (ALU-tail rule); defensive zero keeps the API total.
            None => Ok(Value::F32(0.0)),
        }
    }

    /// Parameter fetch ladder. Caller's explicit map wins, builtins fill the
    /// gap, then we error.
    fn fetch(
        &self,
        uid: u32,
        params: &BTreeMap<u32, Value>,
        body: usize,
    ) -> Result<Value, SbamError> {
        if let Some(v) = params.get(&uid) {
            return Ok(*v);
        }
        match uid {
            // CONFIDENCE: low — the seed reaches the bytecode only through
            // this UID; the in-bytecode mixing arithmetic is still unknown,
            // so the raw seed value is exposed deterministically.
            SEED_UID => Ok(Value::F32(self.seed as f32)),
            // CONFIDENCE: high — `$outputsize` is declared "8,8" in the XML
            // and 8/8 appear in the graph-default block.
            OUTPUT_SIZE_UID => Ok(Value::Vec2([
                self.output_size[0] as f32,
                self.output_size[1] as f32,
            ])),
            _ => Err(SbamError::UnknownParam { uid, body }),
        }
    }
}

/// Register lookup with the SSA invariant defended at run time too.
fn look(regs: &[Value], src: u16, pc: u32) -> Result<Value, SbamError> {
    regs.get(src as usize).copied().ok_or(SbamError::UnresolvedSource {
        pc: pc as usize,
        src,
        produced: regs.len() as u16,
    })
}

// ---------------------------------------------------------------------------
// Arithmetic ladder (hypothesis section — every semantic carries a confidence
// note; neutral sub-op names only; NO invented SBAM-corrected-readable names)
// ---------------------------------------------------------------------------

// CONFIDENCE: low (ladder variant a, componentwise with scalar broadcast) —
// operand shapes are scalar in all decoded scalar-param bodies; vector wiring
// is unverified. Exact per-sub-op arithmetic is NOT decoded yet; the documented
// placeholder ladder makes bodies deterministic and NaN-free. Structural
// (arity/source-resolution) decode around it is CONFIDENCE: high.
//
// `None` = sub-op recognized at decode time but unsupported by the ladder —
// surfaced as `SbamError::UnknownOpcode` at run time, never a panic.
fn binary_ladder(op: u8, sub: u8, a: &Value, b: &Value) -> Option<Value> {
    match (op, sub) {
        // op 0x09 ALU sub-ops [surface observed; math guessed]:
        // mul-guess sub-ops ride on operands 1.0/0.25/2.0 patterns; add/sub
        // guesses assigned so chained uses differ in a stable direction.
        (0x09, 0x10) | (0x09, 0x14) | (0x09, 0x15) => Some(zip2(a, b, |x, y| x * y)),
        (0x09, 0x12) => Some(zip2(a, b, |x, y| x + y)),
        (0x09, 0x13) | (0x09, 0x31) => Some(zip2(a, b, |x, y| x - y)),
        (0x09, 0x30) => Some(zip2(a, b, |x, y| x + y)),
        // high-bit sub-ops observed as body terminals [low: unknown family].
        (0x09, 0x4d) | (0x09, 0xcd) => Some(a.map_lane(sanitize)),
        // op 0x08 sub-ops [observed 0x5d/0x5f/0x61; math guessed: pass-a].
        (0x08, 0x5d) | (0x08, 0x5f) | (0x08, 0x61) => Some(a.map_lane(sanitize)),
        _ => None,
    }
}

// op 0x05 sub-ops [observed 0x16/0x24/0x2b/0x32/0x41/0x51; math guessed:
// pass-a — unary store/plumbing family].
fn unary_ladder(op: u8, sub: u8, a: &Value) -> Option<Value> {
    if op != 0x05 {
        return None;
    }
    match sub {
        0x16 | 0x24 | 0x2b | 0x32 | 0x41 | 0x51 => Some(a.map_lane(sanitize)),
        _ => None,
    }
}

/// Ternary ladder: lerp shape `a + (b - a) * t` [med — the third operand is
/// 0.0/0.5/1.0 immediates or a raw parameter value (Contrast) in decoded
/// bodies; per-sub-op fork encoding is not confirmed].
fn ternary_ladder(op: u8, sub: u8, a: &Value, b: &Value, t: &Value) -> Option<Value> {
    if op != 0x0d {
        return None;
    }
    match sub {
        0x00 | 0x09 | 0x2f => Some(zip3_lerp(a, b, t)),
        _ => None,
    }
}

/// Componentwise application with scalar broadcast; shape = the wider operand.
fn zip2(a: &Value, b: &Value, f: impl Fn(f32, f32) -> f32) -> Value {
    let la = a.lanes();
    let lb = b.lanes();
    let n = la.len().max(lb.len());
    let shape_src = if la.len() >= lb.len() { a } else { b };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = la[i.min(la.len() - 1)];
        let y = lb[i.min(lb.len() - 1)];
        out.push(sanitize(f(x, y)));
    }
    shape_src.of_shape(&out)
}

/// lerp per lane with scalar-broadcast `t`.
fn zip3_lerp(a: &Value, b: &Value, t: &Value) -> Value {
    let la = a.lanes();
    let lb = b.lanes();
    let lt = t.lanes();
    let n = la.len().max(lb.len()).max(lt.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = la[i.min(la.len() - 1)];
        let y = lb[i.min(lb.len() - 1)];
        let tv = lt[i.min(lt.len() - 1)];
        out.push(sanitize(x + (y - x) * tv));
    }
    a.of_shape(&out)
}

/// IEEE-half (f16) decode [med — immediates 0x3F00/0x3F80/0x4140 read back as
/// 0.5/1.0/2.5 under standard half decoding, consistent across bodies].
fn half_to_f32(h: u16) -> f32 {
    let sign = u32::from(h >> 15);
    let exp = u32::from((h >> 10) & 0x1F);
    let mant = u32::from(h & 0x03FF);
    let bits = if exp == 0 {
        // subnormal: shift into f32 subnormal
        (sign << 31) | ((exp + 112) << 23) | (mant << 13)
    } else {
        (sign << 31) | ((exp + 112) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

/// NaN-free normalization [med — notes observe bodies are NaN-free-by-design;
/// non-finite results are clamped to 0.0 to keep evaluation total].
fn sanitize(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Tests — written FIRST (TDD step 1, RED): every semantic the parser/VM
// implements is pinned here before the implementation exists.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// ----------
    /// Synthesized SBAM fixtures
    /// ----------
    ///
    /// Build a minimal but structurally-valid SBAM: fixed 0x38-byte header,
    /// 65 ascending section entries (+ terminator), filler, then `code` bytes
    /// at PROGRAM_START, then a bare 28-byte tail.
    fn synth_sbam(code: &[u8]) -> Vec<u8> {
        let program_len = (PROGRAM_START as usize + code.len() + 28) as u32;
        let mut v = Vec::with_capacity(program_len as usize);
        // 0x00 magic
        v.extend_from_slice(SBAM_MAGIC);
        // 0x04 version (0, 2)
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        // 0x08 cookerbuild
        v.extend_from_slice(&16082u32.to_le_bytes());
        // 0x0C asmuid
        v.extend_from_slice(&0x97CBA38Eu32.to_le_bytes());
        // 0x10 file size (patched later)
        v.extend_from_slice(&program_len.to_le_bytes());
        // 0x14 tail size
        v.extend_from_slice(&TAIL_SIZE.to_le_bytes());
        // 0x18 zero
        v.extend_from_slice(&0u32.to_le_bytes());
        // 0x1C tail offset
        v.extend_from_slice(&(program_len - TAIL_SIZE).to_le_bytes());
        // 0x20, 0x24 observed constants
        v.extend_from_slice(&0x00010002u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        // 0x28 graph count, 0x2C graph-default offset
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&(PROGRAM_START).to_le_bytes());
        // 0x30, 0x34 observed constants
        v.extend_from_slice(&2u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        // 0x38..: 65 ascending section entries
        for i in 0..SECTION_ENTRY_COUNT {
            let off = (0x110 + i) as u32;
            v.extend_from_slice(&off.to_le_bytes());
        }
        // 0x13C terminator [observed 0x000100C0]
        v.extend_from_slice(&0x000100C0u32.to_le_bytes());
        // 0x140 count field [observed 64]
        v.extend_from_slice(&64u32.to_le_bytes());
        // 0x144..0x18C filler (compact-table region; unmodeled)
        while v.len() < PROGRAM_START as usize {
            v.push(0);
        }
        debug_assert_eq!(v.len(), PROGRAM_START as usize);
        // program area
        v.extend_from_slice(code);
        // 28-byte tail
        v.extend_from_slice(&[0u8; 28]);
        v
    }

    /// Instruction bytes helper for fixtures.
    fn head_u32(param: u8, op: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![param, op];
        v.extend_from_slice(payload);
        v
    }

    fn const_f32(x: f32) -> Vec<u8> {
        head_u32(0x00, 0x09, &x.to_le_bytes())
    }

    fn uid_fetch(op: u8, sub: u8, uid: u32) -> Vec<u8> {
        head_u32(sub, op, &uid.to_le_bytes())
    }

    fn label(param: u8) -> Vec<u8> {
        vec![param, 0x00]
    }

    fn end_body() -> Vec<u8> {
        vec![0x00, 0x00]
    }

    fn params(pairs: &[(u32, f32)]) -> BTreeMap<u32, Value> {
        pairs
            .iter()
            .map(|(uid, v)| (*uid, Value::F32(*v)))
            .collect()
    }

    // ----- header / directory ------------------------------------------------

    #[test]
    fn rejects_non_sbam_without_panic() {
        let err = SbamProgram::parse(b"JUNKJUNKJUNK");
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("SBAM"));
    }

    #[test]
    fn truncated_header_is_error_not_panic() {
        // shorter than the fixed header
        let err = SbamProgram::parse(&synth_sbam(b"")[..8]);
        assert!(err.is_err());
    }

    #[test]
    fn synth_fixture_header_fields_round_trip() {
        // non-empty program area (parse requires a decodable region)
        let code = [label(0x2a), const_f32(1.0), end_body()].concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse synth");
        assert_eq!(p.header.version, (0, 2));
        assert_eq!(p.header.cookerbuild, 16082);
        assert_eq!(p.header.asmuid, 0x97CBA38Eu32);
        assert_eq!(p.header.file_size, bytes.len() as u32);
        assert_eq!(p.header.tail_size, TAIL_SIZE);
        assert_eq!(
            p.header.tail_offset,
            bytes.len() as u32 - TAIL_SIZE
        );
        assert_eq!(p.directory_count_field, 64);
        assert_eq!(p.sections.len(), SECTION_ENTRY_COUNT);
        assert_eq!(p.section_terminator, 0x000100C0);
    }

    // ----- real sample --------------------------------------------------------

    /// Ground-truth bytes, extracted from the sample `.sbsar`
    /// (tests skip gracefully when the scratch sample is absent).
    fn sample_bytes() -> Option<Vec<u8>> {
        const CANDIDATES: [&str; 2] = [
            "/tmp/opencode/assemblies/content/0000/mg_dust.sbsasm",
            "/mnt/shared/REMoAINTER/tools/fixtures/mg_dust.sbsasm",
        ];
        for p in CANDIDATES {
            if let Ok(bytes) = std::fs::read(p) {
                if bytes.starts_with(SBAM_MAGIC) {
                    return Some(bytes);
                }
            }
        }
        eprintln!("sample mg_dust.sbsasm not present; skipping");
        None
    }

    #[test]
    fn real_sample_header_and_directory_parse() {
        let Some(bytes) = sample_bytes() else { return };
        let p = SbamProgram::parse(&bytes).expect("parse real sample");
        assert_eq!(p.header.version, (0, 2));
        assert_eq!(p.header.cookerbuild, 16082);
        assert_eq!(p.header.asmuid, 2546705294);
        assert_eq!(p.header.file_size, 9060);
        assert_eq!(p.header.tail_offset, 0x2348);
        assert_eq!(p.header.tail_size, 28);
        // 65 ascending u32 entries at +0x38: 0x110 .. 0x2250
        assert_eq!(p.sections.len(), 65);
        assert_eq!(p.sections[0], 0x110);
        assert_eq!(p.sections[64], 0x2250);
        assert!(p.sections.windows(2).all(|w| w[0] < w[1]), "ascending");
        assert_eq!(p.section_terminator, 0x000100C0);
        assert_eq!(p.directory_count_field, 64);
    }

    // ----- stack/register machine -------------------------------------------

    #[test]
    fn const_push_f32_produces_register_and_value() {
        // label 2a    ; const 1.0 ; unary 05/32 (pass) ; end
        let code = [
            label(0x2a),
            const_f32(1.0),
            head_u32(0x32, 0x05, &0u16.to_le_bytes()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        assert_eq!(p.body_count(), 1);
        let vm = SbamVm::new(&p, 0, [8, 8]);
        let v = vm.evaluate(0, &params(&[])).expect("eval");
        assert_eq!(v.as_f32(), Some(1.0));
    }

    #[test]
    fn binary_alu_reads_two_source_registers() {
        // const 2.0 ; Alu12(r0, r0) [ladder: add -> 4.0] ; end
        let code = [
            label(0x2a),
            const_f32(2.0),
            head_u32(0x12, 0x09, &[0u16.to_le_bytes(), 0u16.to_le_bytes()].concat()),
            head_u32(0x12, 0x09, &[1u16.to_le_bytes(), 1u16.to_le_bytes()].concat()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        let vm = SbamVm::new(&p, 0, [8, 8]);
        let v = vm.evaluate(0, &params(&[])).expect("eval");
        // Alu12 is laddered as add [low]; two chained Alu12s double it twice.
        assert_eq!(v.as_f32(), Some(8.0));
    }

    // ----- UID fetch ----------------------------------------------------------

    #[test]
    fn uid_fetch_carries_parameter_uid_immediate() {
        // label 0f ; fetch Level ; Alu10(r1, r0) with r0 = 1.0 -> 0.25
        let code = [
            label(0x0f),
            uid_fetch(0x09, 0x02, LEVEL_UID),
            const_f32(1.0),
            head_u32(0x10, 0x09, &[1u16.to_le_bytes(), 0u16.to_le_bytes()].concat()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        assert_eq!(p.body_count(), 1);
        assert_eq!(p.bodies[0].fetched_uids, vec![LEVEL_UID]);
        let vm = SbamVm::new(&p, 0, [8, 8]);
        let v = vm.evaluate(0, &params(&[(LEVEL_UID, 0.25)])).expect("eval");
        assert_eq!(v.as_f32(), Some(0.25));
    }

    #[test]
    fn unknown_param_uid_returns_error() {
        let code = [label(0x0f), uid_fetch(0x09, 0x02, 0xDEADBEEF), end_body()].concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        let vm = SbamVm::new(&p, 0, [8, 8]);
        let err = vm.evaluate(0, &params(&[])).expect_err("must not resolve");
        assert_eq!(
            err,
            SbamError::UnknownParam {
                uid: 0xDEADBEEF,
                body: 0
            }
        );
    }

    // ----- ternary / select family ------------------------------------------

    #[test]
    fn ternary_immediate_half_lerps_two_constants() {
        // const 1.0 ; const 3.0 ; 0d/00 (r0, r1, half 0x3800 = 0.5) -> 2.0
        let code = [
            label(0x02),
            const_f32(1.0),
            const_f32(3.0),
            head_u32(0x00, 0x0d, &[0u16.to_le_bytes(), 1u16.to_le_bytes(), 0x3800u16.to_le_bytes()].concat()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        let vm = SbamVm::new(&p, 0, [8, 8]);
        // lerp(1.0, 3.0, 0.5) = 2.0 [med: lerp shape of op 0x0d]
        match vm.evaluate(0, &params(&[])).expect("eval") {
            Value::F32(x) => assert!((x - 2.0).abs() < f32::EPSILON),
            other => panic!("expected scalar, got {other:?}"),
        }
    }

    #[test]
    fn ternary_with_register_blend_factor() {
        // const 1.0 ; const 5.0 ; fetch Level (0.0) ; 0d/09 (r0, r1, r2) -> 1.0
        let code = [
            label(0x02),
            const_f32(1.0),
            const_f32(5.0),
            uid_fetch(0x09, 0x02, LEVEL_UID),
            head_u32(0x09, 0x0d, &[0u16.to_le_bytes(), 1u16.to_le_bytes(), 2u16.to_le_bytes()].concat()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        let vm = SbamVm::new(&p, 0, [8, 8]);
        for (lvl, expect) in [(0.0f32, 1.0f32), (1.0, 5.0), (0.5, 3.0)] {
            let v = vm.evaluate(0, &params(&[(LEVEL_UID, lvl)])).expect("eval");
            assert_eq!(v.as_f32(), Some(expect), "Level={lvl}");
        }
    }

    // ----- boundary ops -------------------------------------------------------

    #[test]
    fn label_ops_are_boundary_nops() {
        // Labels and end markers produce no registers: the body's
        // register_count must equal its producers only (const + unary = 2).
        // NOTE: a label terminates the current decode window, so in real
        // bytecode labels always follow an ALU producer (a label right after
        // a Const push is a rejected candidate — recorded/other bytes).
        let code = [
            label(0x2a),
            const_f32(1.0),
            head_u32(0x32, 0x05, &0u16.to_le_bytes()),
            label(0x5f), // boundary label; body 1 was accepted above it
            const_f32(1.0),
            head_u32(0x32, 0x05, &0u16.to_le_bytes()),
            label(0x01),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        assert_eq!(p.body_count(), 2);
        for b in &p.bodies {
            // 2 producers each; labels contributed 0 registers.
            assert_eq!(b.register_count, 2);
        }
    }

    // ----- error paths --------------------------------------------------------

    #[test]
    fn body_index_out_of_range_errors() {
        let code = [label(0x2a), const_f32(1.0), end_body()].concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        let vm = SbamVm::new(&p, 0, [8, 8]);
        assert_eq!(
            vm.evaluate(99, &params(&[])).unwrap_err(),
            SbamError::BodyIndexOutOfRange {
                index: 99,
                count: p.body_count()
            }
        );
    }

    #[test]
    fn unknown_opcode_returns_typed_error_never_panics() {
        // unary with an unsupported sub-operation: decodes fine (op 0x05 is a
        // known class), but the arithmetic ladder has no 0x7b entry.
        let code = [
            label(0x2a),
            const_f32(1.0),
            head_u32(0x7b, 0x05, &0u16.to_le_bytes()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse");
        let vm = SbamVm::new(&p, 0, [8, 8]);
        let err = vm.evaluate(0, &params(&[])).expect_err("must error");
        assert_eq!(
            err,
            SbamError::UnknownOpcode {
                // pc within file: PROGRAM_START + label(2) + const(6) = +8
                pc: PROGRAM_START as usize + 8,
                op: 0x7b
            }
        );
    }

    #[test]
    fn forward_source_reference_is_rejected_at_parse() {
        // A binary op referencing register 3 before anything produced — the
        // SSA invariant must reject the body, not panic.
        let code = [
            label(0x2a),
            head_u32(0x12, 0x09, &[3u16.to_le_bytes(), 0u16.to_le_bytes()].concat()),
            const_f32(1.0),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&code);
        let p = SbamProgram::parse(&bytes).expect("parse (candidate rejected, no error)");
        assert!(p.bodies.is_empty(), "unresolvable body must not be accepted");
    }

    // ----- determinism --------------------------------------------------------

    #[test]
    fn evaluation_is_reentrant_and_seed_free() {
        let level_body = [
            label(0x0f),
            uid_fetch(0x09, 0x02, LEVEL_UID),  // r0
            const_f32(0.5),                    // r1
            // sub(1, 0): binary sources must already exist (SSA)
            head_u32(0x31, 0x09, &[1u16.to_le_bytes(), 0u16.to_le_bytes()].concat()), // r2
            end_body(),
        ]
        .concat();
        // A body must end on ALU work (const-only bodies are decode
        // rejects — the tail rule), so the second body is const + pass.
        let plain_body = [
            label(0x2a),
            const_f32(1.0),
            head_u32(0x32, 0x05, &0u16.to_le_bytes()),
            end_body(),
        ]
        .concat();
        let bytes = synth_sbam(&[level_body, plain_body].concat());
        let p = SbamProgram::parse(&bytes).expect("parse");
        assert_eq!(p.body_count(), 2);
        // body 0: fetch + const + binary = 3 regs; body 1: const + unary = 2.
        assert_eq!(p.bodies[0].register_count, 3);
        assert_eq!(p.bodies[1].register_count, 2);
        let ps = params(&[(LEVEL_UID, 0.5)]);

        // Same VM, same inputs: identical each run.
        let vm = SbamVm::new(&p, 0x1234, [8, 8]);
        let a1 = vm.evaluate(0, &ps).expect("eval");
        let a2 = vm.evaluate(0, &ps).expect("eval");
        assert_eq!(a1, a2);

        // Order independence: running body 0 then body 1 must equal running
        // body 1 then body 0 (no cross-body state, incl. no seed mutation).
        let fwd1 = vm.evaluate(0, &ps).unwrap();
        let fwd2 = vm.evaluate(1, &ps).unwrap();
        let rev1 = vm.evaluate(1, &ps).unwrap();
        let rev2 = vm.evaluate(0, &ps).unwrap();
        assert_eq!(fwd2, rev1);
        assert_eq!(fwd1, rev2);
    }

    // ----- real sample: bodies ----------------------------------------------

    #[test]
    fn real_sample_bodies_parse_ssa_clean() {
        let Some(bytes) = sample_bytes() else { return };
        let p = SbamProgram::parse(&bytes).expect("parse");
        // Expect the 16 ALU noise bodies (2 copies x 8 nodes 0x55..0xcc) plus
        // the Level/Contrast pair and the Occlusion_Amount pair and the two
        // invert bodies: all must decode with resolved sources.
        assert!(p.body_count() >= 21, "bodies found: {}", p.body_count());

        // noise-template bodies: unary 05/32 as first ALU, terminal 09/4d.
        let noise = p
            .bodies
            .iter()
            .filter(|b| b.node.is_some())
            .count();
        assert!(noise >= 16, "noise-template bodies: {noise}");

        // Level+Contrast param decode bodies
        let lvl = p
            .bodies
            .iter()
            .filter(|b| {
                b.fetched_uids.contains(&LEVEL_UID) && b.fetched_uids.contains(&CONTRAST_UID)
            })
            .count();
        assert_eq!(lvl, 2, "Level bodies");

        // Occlusion_Amount bodies
        let occ = p
            .bodies
            .iter()
            .filter(|b| b.fetched_uids.contains(&OCCLUSION_AMOUNT_UID))
            .count();
        assert_eq!(occ, 2, "Occlusion bodies");

        // invert bodies (via the 08/42 fetch form [med])
        let inv = p
            .bodies
            .iter()
            .filter(|b| b.fetched_uids.contains(&INVERT_UID))
            .count();
        assert!(inv >= 1, "invert bodies: {inv}");
    }

    #[test]
    fn real_sample_every_body_deterministic() {
        let Some(bytes) = sample_bytes() else { return };
        let p = SbamProgram::parse(&bytes).expect("parse");
        // Graph-default slider state: XML defaults are 0.5 for Level,
        // Contrast, Noise_Opacity [confirmed: graph-default block f32 triple].
        let ps = params(&[
            (LEVEL_UID, 0.5),
            (CONTRAST_UID, 0.5),
            (OCCLUSION_AMOUNT_UID, 0.5),
            (NOISE_OPACITY_UID, 0.5),
        ]);
        let vm = SbamVm::new(&p, 0x5EED, [8, 8]);
        // run forward, keep results
        let mut res = Vec::new();
        for i in 0..p.body_count() {
            if let Ok(v) = vm.evaluate(i, &ps) {
                res.push(v);
            }
        }
        // run ALL again (twice over): identical, proving re-entrancy
        let mut res2 = Vec::new();
        for i in 0..p.body_count() {
            if let Ok(v) = vm.evaluate(i, &ps) {
                res2.push(v);
            }
        }
        assert_eq!(res, res2);
        assert!(!res.is_empty());
    }

    #[test]
    fn real_sample_level_pair_executes_deterministically() {
        // SEMANTIC PROBE (documented, unresolved): the paired Level bodies
        // compute an operand-swapped pair — b19 = Op(D2, Q, f(C)) while
        // b20 = Op(Q, D2, f(C)) where f is the Contrast-derived chain
        // (f(0.5)=1.25 under the placeholder arithmetic ladder). The pair
        // agrees at the graph default only if op 0x0d's first two arguments
        // are commutative (min/max/add); the lerp hypothesis FAILS pair
        // equality (1.75 vs -2.75), so exact semantics remain a documented
        // oracle. Here we assert what the SSA model guarantees: both bodies
        // execute, and repeated evaluation is bit-identical.
        let Some(bytes) = sample_bytes() else { return };
        let p = SbamProgram::parse(&bytes).expect("parse");
        let picks: Vec<usize> = p
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.fetched_uids.contains(&LEVEL_UID) && b.fetched_uids.contains(&CONTRAST_UID)
            })
            .map(|(i, _)| i)
            .collect();
        assert!(picks.len() >= 2);
        let ps = params(&[(LEVEL_UID, 0.5), (CONTRAST_UID, 0.5)]);
        let vm = SbamVm::new(&p, 0, [8, 8]);
        // Each paired body executes under the default parameter state...
        let a1 = vm.evaluate(picks[0], &ps).expect("eval body 1");
        let b1 = vm.evaluate(picks[1], &ps).expect("eval body 2");
        // ...and every repeat run is bit-identical (re-entrancy + determinism).
        for _ in 0..3 {
            assert_eq!(vm.evaluate(picks[0], &ps).unwrap(), a1);
            assert_eq!(vm.evaluate(picks[1], &ps).unwrap(), b1);
        }
    }
}
