/// 簡易乱数生成器（xorshift）
pub struct Rng {
    x: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { x: if seed == 0 { 123456789 } else { seed } }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.x ^= self.x << 13;
        self.x ^= self.x >> 7;
        self.x ^= self.x << 17;
        self.x
    }

    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() % 1_000_000) as f64 / 1_000_000.0
    }

    pub fn ri(&mut self, min: i8, max: i8) -> i8 {
        let range = (max - min + 1) as u64;
        (self.next_u64() % range) as i8 + min
    }

    pub fn pick<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        let idx = self.next_u64() as usize % arr.len();
        &arr[idx]
    }

    pub fn shuffle<T>(&mut self, arr: &mut [T]) {
        for i in (1..arr.len()).rev() {
            let j = self.next_u64() as usize % (i + 1);
            arr.swap(i, j);
        }
    }
}
