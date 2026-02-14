//! Arbitrary-precision integer arithmetic for BigNum values.
//!
//! All operations work on u32 limbs in little-endian order (least significant first).
//! Base is 2^32. No heap allocation - uses fixed-size stack arrays.

/// Maximum number of limbs supported (128 limbs = 4096 bits)
pub const MAX_LIMBS: usize = 128;

/// BigNum represented as an array of u32 limbs (little-endian, base 2^32).
#[derive(Clone, Copy)]
pub struct BigNumBuf {
    pub limbs: [u32; MAX_LIMBS],
    pub len: usize,
    pub negative: bool,
}

impl BigNumBuf {
    /// Create a zero BigNum.
    pub const fn zero() -> Self {
        Self {
            limbs: [0; MAX_LIMBS],
            len: 0,
            negative: false,
        }
    }

    /// Create BigNum from an isize value.
    pub fn from_isize(n: isize) -> Self {
        if n == 0 {
            return Self::zero();
        }
        let negative = n < 0;
        let abs_val = n.unsigned_abs() as u64;
        let mut buf = Self::zero();
        buf.negative = negative;
        buf.limbs[0] = abs_val as u32;
        buf.limbs[1] = (abs_val >> 32) as u32;
        buf.len = if buf.limbs[1] > 0 { 2 } else { 1 };
        buf
    }

    /// Create BigNum from a u64 value.
    pub fn from_u64(n: u64, negative: bool) -> Self {
        if n == 0 {
            return Self::zero();
        }
        let mut buf = Self::zero();
        buf.negative = negative;
        buf.limbs[0] = n as u32;
        buf.limbs[1] = (n >> 32) as u32;
        buf.len = if buf.limbs[1] > 0 { 2 } else { 1 };
        buf
    }

    /// Trim leading zeros.
    pub fn trim(&mut self) {
        while self.len > 0 && self.limbs[self.len - 1] == 0 {
            self.len -= 1;
        }
        if self.len == 0 {
            self.negative = false;
        }
    }

    /// Check if this BigNum is zero.
    pub fn is_zero(&self) -> bool {
        self.len == 0
    }

    /// Try to convert to isize. Returns None if too large.
    pub fn to_isize(&self) -> Option<isize> {
        if self.len == 0 {
            return Some(0);
        }
        if self.len == 1 {
            let v = self.limbs[0] as isize;
            return Some(if self.negative { -v } else { v });
        }
        if self.len == 2 {
            let v = self.limbs[0] as u64 | ((self.limbs[1] as u64) << 32);
            if v <= isize::MAX as u64 {
                let sv = v as isize;
                return Some(if self.negative { -sv } else { sv });
            }
            if self.negative && v == (isize::MAX as u64) + 1 {
                return Some(isize::MIN);
            }
        }
        None
    }

    /// Convert to f64 (lossy).
    pub fn to_f64(&self) -> f64 {
        if self.len == 0 {
            return 0.0;
        }
        let mut result: f64 = 0.0;
        let base: f64 = (1u64 << 32) as f64;
        let mut multiplier: f64 = 1.0;
        for i in 0..self.len {
            result += self.limbs[i] as f64 * multiplier;
            multiplier *= base;
        }
        if self.negative {
            -result
        } else {
            result
        }
    }

    /// Compare magnitudes (ignoring sign). Returns Ordering.
    pub fn cmp_magnitude(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        if self.len != other.len {
            return self.len.cmp(&other.len);
        }
        for i in (0..self.len).rev() {
            if self.limbs[i] != other.limbs[i] {
                return self.limbs[i].cmp(&other.limbs[i]);
            }
        }
        Ordering::Equal
    }

    /// Compare two BigNums (with sign).
    pub fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        if self.is_zero() && other.is_zero() {
            return Ordering::Equal;
        }
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.cmp_magnitude(other),
            (true, true) => other.cmp_magnitude(self), // reversed for negatives
        }
    }

    /// Add two BigNums (handling signs).
    pub fn add(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            // Same sign: add magnitudes, keep sign
            let mut result = add_magnitudes(self, other);
            result.negative = self.negative;
            result
        } else {
            // Different signs: subtract smaller from larger
            match self.cmp_magnitude(other) {
                core::cmp::Ordering::Greater | core::cmp::Ordering::Equal => {
                    let mut result = sub_magnitudes(self, other);
                    result.negative = self.negative;
                    result.trim();
                    result
                }
                core::cmp::Ordering::Less => {
                    let mut result = sub_magnitudes(other, self);
                    result.negative = other.negative;
                    result.trim();
                    result
                }
            }
        }
    }

    /// Subtract other from self.
    pub fn sub(&self, other: &Self) -> Self {
        let mut neg_other = *other;
        neg_other.negative = !other.negative;
        if neg_other.is_zero() {
            neg_other.negative = false;
        }
        self.add(&neg_other)
    }

    /// Multiply two BigNums.
    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut result = Self::zero();
        result.negative = self.negative != other.negative;
        let result_len = self.len + other.len;
        if result_len > MAX_LIMBS {
            // Overflow - set to max
            result.len = MAX_LIMBS;
            return result;
        }
        result.len = result_len;

        for i in 0..self.len {
            let mut carry: u64 = 0;
            for j in 0..other.len {
                if i + j >= MAX_LIMBS {
                    break;
                }
                let prod = self.limbs[i] as u64 * other.limbs[j] as u64
                    + result.limbs[i + j] as u64
                    + carry;
                result.limbs[i + j] = prod as u32;
                carry = prod >> 32;
            }
            if i + other.len < MAX_LIMBS {
                result.limbs[i + other.len] = carry as u32;
            }
        }
        result.trim();
        result
    }

    /// Divide self by a single u32 digit. Returns (quotient, remainder).
    pub fn div_u32(&self, divisor: u32) -> (Self, u32) {
        if divisor == 0 {
            // Division by zero - return zero
            return (Self::zero(), 0);
        }
        let mut quotient = Self::zero();
        quotient.negative = self.negative;
        quotient.len = self.len;
        let mut rem: u64 = 0;
        for i in (0..self.len).rev() {
            let cur = rem * (1u64 << 32) + self.limbs[i] as u64;
            quotient.limbs[i] = (cur / divisor as u64) as u32;
            rem = cur % divisor as u64;
        }
        quotient.trim();
        (quotient, rem as u32)
    }

    /// Full bignum division: self / other. Returns (quotient, remainder).
    pub fn divmod(&self, other: &Self) -> (Self, Self) {
        if other.is_zero() {
            return (Self::zero(), Self::zero());
        }
        if self.is_zero() {
            return (Self::zero(), Self::zero());
        }

        let cmp = self.cmp_magnitude(other);
        if cmp == core::cmp::Ordering::Less {
            return (Self::zero(), *self);
        }
        if cmp == core::cmp::Ordering::Equal {
            let mut one = Self::zero();
            one.limbs[0] = 1;
            one.len = 1;
            one.negative = self.negative != other.negative;
            let mut zero = Self::zero();
            zero.negative = false;
            return (one, zero);
        }

        // Use long division for multi-limb divisors
        if other.len == 1 {
            let (q, r) = self.div_u32(other.limbs[0]);
            let mut q_result = q;
            q_result.negative = self.negative != other.negative;
            let mut r_result = BigNumBuf::from_u64(r as u64, self.negative);
            r_result.trim();
            return (q_result, r_result);
        }

        // Knuth Algorithm D (long division)
        bignum_long_division(self, other)
    }

    /// Negate this BigNum.
    pub fn negate(&self) -> Self {
        let mut result = *self;
        if !result.is_zero() {
            result.negative = !result.negative;
        }
        result
    }

    /// Shift left by n bits.
    pub fn shl(&self, n: u32) -> Self {
        if self.is_zero() || n == 0 {
            return *self;
        }
        let word_shift = (n / 32) as usize;
        let bit_shift = n % 32;
        let mut result = Self::zero();
        result.negative = self.negative;

        let new_len = self.len + word_shift + if bit_shift > 0 { 1 } else { 0 };
        if new_len > MAX_LIMBS {
            result.len = MAX_LIMBS;
            return result;
        }

        if bit_shift == 0 {
            for i in 0..self.len {
                if i + word_shift < MAX_LIMBS {
                    result.limbs[i + word_shift] = self.limbs[i];
                }
            }
        } else {
            let mut carry: u32 = 0;
            for i in 0..self.len {
                let shifted = ((self.limbs[i] as u64) << bit_shift) | carry as u64;
                if i + word_shift < MAX_LIMBS {
                    result.limbs[i + word_shift] = shifted as u32;
                }
                carry = (shifted >> 32) as u32;
            }
            if self.len + word_shift < MAX_LIMBS && carry > 0 {
                result.limbs[self.len + word_shift] = carry;
            }
        }
        result.len = new_len.min(MAX_LIMBS);
        result.trim();
        result
    }

    /// Shift right by n bits.
    pub fn shr(&self, n: u32) -> Self {
        if self.is_zero() || n == 0 {
            return *self;
        }
        let word_shift = (n / 32) as usize;
        let bit_shift = n % 32;

        if word_shift >= self.len {
            return Self::zero();
        }

        let mut result = Self::zero();
        result.negative = self.negative;

        if bit_shift == 0 {
            for i in word_shift..self.len {
                result.limbs[i - word_shift] = self.limbs[i];
            }
        } else {
            for i in word_shift..self.len {
                result.limbs[i - word_shift] = self.limbs[i] >> bit_shift;
                if i + 1 < self.len {
                    result.limbs[i - word_shift] |= self.limbs[i + 1] << (32 - bit_shift);
                }
            }
        }
        result.len = self.len - word_shift;
        result.trim();
        result
    }

    /// Count the number of significant bits.
    pub fn bit_length(&self) -> u32 {
        if self.len == 0 {
            return 0;
        }
        let top = self.limbs[self.len - 1];
        (self.len as u32 - 1) * 32 + (32 - top.leading_zeros())
    }

    /// Integer square root using Newton's method with BigNum arithmetic.
    pub fn isqrt(&self) -> Self {
        if self.is_zero() || self.negative {
            return Self::zero();
        }
        if self.len == 1 && self.limbs[0] <= 1 {
            return *self;
        }

        // Initial estimate: 2^(ceil(bit_length/2))
        let bits = self.bit_length();
        let mut x = Self::zero();
        let shift_bits = (bits + 1) / 2;
        let word_idx = (shift_bits / 32) as usize;
        let bit_idx = shift_bits % 32;
        if word_idx < MAX_LIMBS {
            x.limbs[word_idx] = 1u32 << bit_idx;
            x.len = word_idx + 1;
        }

        // Newton's iteration: x = (x + n/x) / 2
        loop {
            let (q, _) = self.divmod(&x);
            let sum = x.add(&q);
            let (new_x, _) = sum.div_u32(2);

            if new_x.cmp_magnitude(&x) != core::cmp::Ordering::Less {
                break;
            }
            x = new_x;
        }
        x
    }
}

/// Add magnitudes of two BigNums (ignoring sign).
fn add_magnitudes(a: &BigNumBuf, b: &BigNumBuf) -> BigNumBuf {
    let mut result = BigNumBuf::zero();
    let max_len = if a.len > b.len { a.len } else { b.len };
    let mut carry: u64 = 0;

    for i in 0..max_len {
        let av = if i < a.len { a.limbs[i] as u64 } else { 0 };
        let bv = if i < b.len { b.limbs[i] as u64 } else { 0 };
        let sum = av + bv + carry;
        result.limbs[i] = sum as u32;
        carry = sum >> 32;
    }
    if carry > 0 && max_len < MAX_LIMBS {
        result.limbs[max_len] = carry as u32;
        result.len = max_len + 1;
    } else {
        result.len = max_len;
    }
    result
}

/// Subtract magnitude of b from a (assumes |a| >= |b|, ignoring sign).
fn sub_magnitudes(a: &BigNumBuf, b: &BigNumBuf) -> BigNumBuf {
    let mut result = BigNumBuf::zero();
    let mut borrow: i64 = 0;

    for i in 0..a.len {
        let av = a.limbs[i] as i64;
        let bv = if i < b.len { b.limbs[i] as i64 } else { 0 };
        let diff = av - bv - borrow;
        if diff < 0 {
            result.limbs[i] = (diff + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            result.limbs[i] = diff as u32;
            borrow = 0;
        }
    }
    result.len = a.len;
    result.trim();
    result
}

/// Knuth's Algorithm D for long division.
fn bignum_long_division(u: &BigNumBuf, v: &BigNumBuf) -> (BigNumBuf, BigNumBuf) {
    let n = v.len;
    let m = u.len - n;

    // Normalize: shift so that v's most significant limb has high bit set
    let shift = v.limbs[n - 1].leading_zeros();
    let un = u.shl(shift);
    let vn = v.shl(shift);

    let mut q = BigNumBuf::zero();
    q.len = m + 1;

    // Work buffer for the remainder (un padded by one extra limb)
    let mut rem_limbs = [0u32; MAX_LIMBS + 1];
    for i in 0..un.len {
        rem_limbs[i] = un.limbs[i];
    }
    let rem_len = if un.len > u.len { un.len } else { un.len + 1 };

    for j in (0..=m).rev() {
        // Estimate q_hat = (rem[j+n]*b + rem[j+n-1]) / vn[n-1]
        let r_top = if j + n < rem_len { rem_limbs[j + n] as u64 } else { 0 };
        let r_next = if j + n >= 1 { rem_limbs[j + n - 1] as u64 } else { 0 };
        let divisor = vn.limbs[n - 1] as u64;

        let mut q_hat = if divisor == 0 {
            u64::MAX
        } else {
            let num = (r_top << 32) | r_next;
            num / divisor
        };

        if q_hat > 0xFFFF_FFFF {
            q_hat = 0xFFFF_FFFF;
        }

        // Refine estimate
        loop {
            let r_hat = (r_top << 32) | r_next;
            let r_hat = r_hat.wrapping_sub(q_hat * divisor);
            if r_hat > 0xFFFF_FFFF {
                break; // Will underflow
            }
            if n >= 2 {
                let v_next = vn.limbs[n - 2] as u64;
                let rem_next = if j + n >= 2 { rem_limbs[j + n - 2] as u64 } else { 0 };
                if q_hat * v_next > (r_hat << 32) + rem_next {
                    q_hat -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Multiply and subtract: rem[j..j+n] -= q_hat * vn
        let mut borrow: i64 = 0;
        for i in 0..n {
            let prod = q_hat * vn.limbs[i] as u64;
            let val = rem_limbs[j + i] as i64 - (prod as u32 as i64) - borrow;
            rem_limbs[j + i] = val as u32;
            borrow = (prod >> 32) as i64 - (val >> 32) as i64;
        }
        if j + n < rem_len {
            let val = rem_limbs[j + n] as i64 - borrow;
            rem_limbs[j + n] = val as u32;
            if val < 0 {
                // q_hat was too large, add back
                q_hat -= 1;
                let mut carry: u64 = 0;
                for i in 0..n {
                    let sum = rem_limbs[j + i] as u64 + vn.limbs[i] as u64 + carry;
                    rem_limbs[j + i] = sum as u32;
                    carry = sum >> 32;
                }
                if j + n < rem_len {
                    rem_limbs[j + n] = rem_limbs[j + n].wrapping_add(carry as u32);
                }
            }
        }

        q.limbs[j] = q_hat as u32;
    }

    q.negative = u.negative != v.negative;
    q.trim();

    // Unshift remainder
    let mut remainder = BigNumBuf::zero();
    for i in 0..n {
        remainder.limbs[i] = rem_limbs[i];
    }
    remainder.len = n;
    remainder.trim();
    let mut remainder = remainder.shr(shift);
    remainder.negative = u.negative;
    remainder.trim();

    (q, remainder)
}
