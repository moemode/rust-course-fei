mod calc {
    pub enum Op {
        Add(i32),
        Sub(i32),
        Clamp { low: i32, high: i32 },
    }

    pub fn perform_calculation(x: i32, op: Op) -> i32 {
        match op {
            Op::Add(y) => x + y,
            Op::Sub(y) => x - y,
            Op::Clamp { low, high } => i32::min(i32::max(x, low), high),
        }
    }
}

pub use calc::{perform_calculation, Op};
