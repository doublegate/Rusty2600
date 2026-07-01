//! A small boolean expression engine for watch entries and conditional
//! breakpoints.
//!
//! Grammar (deliberately minimal — no parentheses, one precedence level):
//! `expr := comparison ( ("&&" | "||") comparison )*`, left to right, `||`
//! given lowest precedence (each `||`-separated group must have ALL its
//! `&&`-joined comparisons true). A `comparison` is `operand op operand`
//! with `op` one of `== != <= >= < >`. An `operand` is a register name
//! (`a x y s pc scanline color_clock frame`), a memory peek (`[$addr]` or
//! `[addr]`), or a numeric literal (`$hex` or decimal).
//!
//! Evaluated against an [`EvalContext`] built once per frame from the live
//! [`super::DebugSnapshot`] plus a peek callback for `[addr]` reads.

/// Everything an expression can reference.
pub struct EvalContext<'a> {
    /// Accumulator.
    pub a: u8,
    /// X index register.
    pub x: u8,
    /// Y index register.
    pub y: u8,
    /// Stack pointer.
    pub s: u8,
    /// Program counter.
    pub pc: u16,
    /// The current scanline.
    pub scanline: u16,
    /// The current color clock within the scanline.
    pub color_clock: u16,
    /// Frames completed since power-on/ROM-load.
    pub frame: u64,
    /// A contiguous memory window backing `[addr]` operands.
    pub mem: &'a [u8],
    /// The address `mem[0]` corresponds to — `[addr]` resolves as
    /// `mem[addr - mem_base]`, reading as `0` outside that window.
    pub mem_base: u16,
}

impl EvalContext<'_> {
    fn peek(&self, addr: u16) -> u8 {
        addr.checked_sub(self.mem_base)
            .and_then(|i| self.mem.get(usize::from(i)))
            .copied()
            .unwrap_or(0)
    }
}

/// Why an expression failed to evaluate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    /// The expression (or one of its `&&`/`||`-joined parts) was empty.
    Empty,
    /// A comparison had no recognized operator.
    NoOperator,
    /// An operand wasn't a known register, a well-formed `[addr]` peek, or
    /// a parsable number.
    BadOperand,
}

fn parse_number(s: &str) -> Result<i64, ExprError> {
    let s = s.trim();
    s.strip_prefix('$').map_or_else(
        || s.parse::<i64>().map_err(|_| ExprError::BadOperand),
        |hex| i64::from_str_radix(hex, 16).map_err(|_| ExprError::BadOperand),
    )
}

fn eval_operand(tok: &str, ctx: &EvalContext) -> Result<i64, ExprError> {
    let tok = tok.trim();
    if let Some(inner) = tok.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let addr = u16::try_from(parse_number(inner)?).map_err(|_| ExprError::BadOperand)?;
        return Ok(i64::from(ctx.peek(addr)));
    }
    match tok {
        "a" => Ok(i64::from(ctx.a)),
        "x" => Ok(i64::from(ctx.x)),
        "y" => Ok(i64::from(ctx.y)),
        "s" => Ok(i64::from(ctx.s)),
        "pc" => Ok(i64::from(ctx.pc)),
        "scanline" => Ok(i64::from(ctx.scanline)),
        "color_clock" => Ok(i64::from(ctx.color_clock)),
        "frame" => i64::try_from(ctx.frame).map_err(|_| ExprError::BadOperand),
        _ => parse_number(tok),
    }
}

/// Operators checked longest-first so `<=`/`>=`/`==`/`!=` never get
/// mis-split by a bare `<`/`>` match.
const OPERATORS: [&str; 6] = ["==", "!=", "<=", ">=", "<", ">"];

fn eval_comparison(cmp: &str, ctx: &EvalContext) -> Result<bool, ExprError> {
    let cmp = cmp.trim();
    if cmp.is_empty() {
        return Err(ExprError::Empty);
    }
    for op in OPERATORS {
        if let Some(idx) = cmp.find(op) {
            let lhs = eval_operand(&cmp[..idx], ctx)?;
            let rhs = eval_operand(&cmp[idx + op.len()..], ctx)?;
            return Ok(match op {
                "==" => lhs == rhs,
                "!=" => lhs != rhs,
                "<=" => lhs <= rhs,
                ">=" => lhs >= rhs,
                "<" => lhs < rhs,
                ">" => lhs > rhs,
                _ => unreachable!("OPERATORS is exhaustively matched above"),
            });
        }
    }
    Err(ExprError::NoOperator)
}

/// Evaluates `expr` against `ctx`. See the module doc for the grammar.
///
/// # Errors
///
/// Returns [`ExprError`] if `expr` (or any `&&`/`||`-joined part of it) is
/// empty, has no recognized comparison operator, or has an operand that
/// isn't a known register, a well-formed `[addr]` peek, or a parsable
/// number.
pub fn evaluate(expr: &str, ctx: &EvalContext) -> Result<bool, ExprError> {
    if expr.trim().is_empty() {
        return Err(ExprError::Empty);
    }
    for or_part in expr.split("||") {
        let mut all_true = true;
        for and_part in or_part.split("&&") {
            if !eval_comparison(and_part, ctx)? {
                all_true = false;
                break;
            }
        }
        if all_true {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(mem: &[u8]) -> EvalContext<'_> {
        EvalContext {
            a: 0x42,
            x: 1,
            y: 2,
            s: 0xFD,
            pc: 0xF000,
            scanline: 10,
            color_clock: 20,
            frame: 5,
            mem,
            mem_base: 0x80,
        }
    }

    #[test]
    fn register_equality() {
        assert_eq!(evaluate("a == $42", &ctx(&[])), Ok(true));
        assert_eq!(evaluate("a == $41", &ctx(&[])), Ok(false));
    }

    #[test]
    fn register_inequality_and_ranges() {
        assert_eq!(evaluate("pc >= $F000", &ctx(&[])), Ok(true));
        assert_eq!(evaluate("pc > $F000", &ctx(&[])), Ok(false));
        assert_eq!(evaluate("y != 3", &ctx(&[])), Ok(true));
    }

    #[test]
    fn memory_peek_operand() {
        let mem = [0x99];
        assert_eq!(evaluate("[$80] == $99", &ctx(&mem)), Ok(true));
        assert_eq!(evaluate("[128] == 153", &ctx(&mem)), Ok(true));
    }

    #[test]
    fn memory_peek_outside_window_reads_zero() {
        let mem = [0x99];
        assert_eq!(evaluate("[$FF] == 0", &ctx(&mem)), Ok(true));
        assert_eq!(evaluate("[$10] == 0", &ctx(&mem)), Ok(true));
    }

    #[test]
    fn scanline_and_color_clock_and_frame() {
        assert_eq!(
            evaluate("scanline == 10 && color_clock == 20", &ctx(&[])),
            Ok(true)
        );
        assert_eq!(evaluate("frame == 5", &ctx(&[])), Ok(true));
    }

    #[test]
    fn logical_and_or() {
        // Neither branch true on its own -> overall false.
        assert_eq!(evaluate("a == 1 && x == 1", &ctx(&[])), Ok(false));
        // Second || branch true -> overall true.
        assert_eq!(evaluate("a == 1 || x == 1", &ctx(&[])), Ok(true));
    }

    #[test]
    fn empty_expression_is_an_error() {
        assert_eq!(evaluate("", &ctx(&[])), Err(ExprError::Empty));
        assert_eq!(evaluate("   ", &ctx(&[])), Err(ExprError::Empty));
    }

    #[test]
    fn missing_operator_is_an_error() {
        assert_eq!(evaluate("a", &ctx(&[])), Err(ExprError::NoOperator));
    }

    #[test]
    fn unknown_operand_is_an_error() {
        assert_eq!(
            evaluate("bogus == 1", &ctx(&[])),
            Err(ExprError::BadOperand)
        );
    }
}
