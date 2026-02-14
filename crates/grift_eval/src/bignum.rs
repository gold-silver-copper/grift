//! Arbitrary-precision integer arithmetic for BigNum values.
//!
//! All operations work on usize limbs in little-endian order (least significant first).
//! Base is 2^LIMB_BITS. No heap allocation - uses fixed-size stack arrays.

/// Maximum number of limbs supported (128 limbs = 4096 bits)
pub const MAX_LIMBS: usize = 128;

/// Number of bits per limb (depends on pointer width).
pub const LIMB_BITS: u32 = core::mem::size_of::<usize>() as u32 * 8;

/// Maximum value that fits in a single limb.
const LIMB_MAX: u128 = (1u128 << LIMB_BITS) - 1;

/// BigNum represented as an array of usize limbs (little-endian, base 2^LIMB_BITS).
#[derive(Clone, Copy)]
pub struct BigNumBuf {
    pub limbs: [usize; MAX_LIMBS],
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
        let abs_val = n.unsigned_abs() as u128;
        let mut buf = Self::zero();
        buf.negative = negative;
        buf.limbs[0] = abs_val as usize;
        buf.limbs[1] = (abs_val >> LIMB_BITS) as usize;
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
        let val = n as u128;
        buf.limbs[0] = val as usize;
        let hi = (val >> LIMB_BITS) as usize;
        buf.limbs[1] = hi;
        buf.len = if hi > 0 { 2 } else { 1 };
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
            let v = self.limbs[0] as u128 | ((self.limbs[1] as u128) << LIMB_BITS);
            if v <= isize::MAX as u128 {
                let sv = v as isize;
                return Some(if self.negative { -sv } else { sv });
            }
            if self.negative && v == (isize::MAX as u128) + 1 {
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
        let base: f64 = (1u128 << LIMB_BITS) as f64;
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

    /// Convert a finite integer f64 to its exact BigNum representation.
    /// Returns None if the float is NaN, infinite, or has a fractional part.
    pub fn from_f64(f: f64) -> Option<Self> {
        if f.is_nan() || f.is_infinite() || f != libm::floor(f) {
            return None;
        }
        if f == 0.0 {
            return Some(Self::zero());
        }
        let negative = f < 0.0;
        let f_abs = libm::fabs(f);

        // Decompose f64: value = mantissa * 2^(exponent - 52)
        let bits = f_abs.to_bits();
        let raw_exp = ((bits >> 52) & 0x7FF) as i32;
        let raw_mantissa = bits & 0x000F_FFFF_FFFF_FFFF;
        // Normalized: mantissa has implicit leading 1
        let mantissa = raw_mantissa | (1u64 << 52);
        // Actual exponent (unbiased, relative to mantissa as integer)
        let exp = raw_exp - 1023 - 52; // exponent for mantissa as 53-bit integer

        // Start with mantissa as BigNum
        let mut result = Self::zero();
        let mantissa128 = mantissa as u128;
        result.limbs[0] = mantissa128 as usize;
        result.limbs[1] = (mantissa128 >> LIMB_BITS) as usize;
        result.len = if result.limbs[1] != 0 { 2 } else { 1 };
        result.negative = negative;

        if exp > 0 {
            // Multiply by 2^exp (left shift)
            result.shl_bits(exp as u32);
        } else if exp < 0 {
            // This should not happen since we checked f == floor(f),
            // but handle it by checking the shifted-out bits are zero
            let shift = (-exp) as u32;
            // Check that the low bits are zero (exact integer)
            if shift >= 64 || (mantissa & ((1u64 << shift) - 1)) != 0 {
                return None;
            }
            // Right shift
            let shifted = (mantissa >> shift) as u128;
            result = Self::zero();
            result.limbs[0] = shifted as usize;
            result.limbs[1] = (shifted >> LIMB_BITS) as usize;
            result.len = if result.limbs[1] != 0 { 2 } else if result.limbs[0] != 0 { 1 } else { 0 };
            result.negative = negative;
        }

        result.trim();
        Some(result)
    }

    /// Left-shift this BigNum by `n` bits (multiply by 2^n).
    fn shl_bits(&mut self, n: u32) {
        if self.len == 0 || n == 0 {
            return;
        }
        let word_shift = (n / LIMB_BITS) as usize;
        let bit_shift = n % LIMB_BITS;
        if word_shift > 0 {
            if self.len + word_shift > MAX_LIMBS {
                return; // overflow - can't represent
            }
            // Move limbs up
            let mut i = self.len;
            while i > 0 {
                i -= 1;
                self.limbs[i + word_shift] = self.limbs[i];
            }
            for j in 0..word_shift {
                self.limbs[j] = 0;
            }
            self.len += word_shift;
        }

        // Shift bits within words
        if bit_shift > 0 {
            let mut carry: usize = 0;
            for i in word_shift..self.len {
                let val = (self.limbs[i] as u128) << bit_shift | carry as u128;
                self.limbs[i] = val as usize;
                carry = (val >> LIMB_BITS) as usize;
            }
            if carry != 0 {
                if self.len < MAX_LIMBS {
                    self.limbs[self.len] = carry;
                    self.len += 1;
                }
            }
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
            let mut carry: u128 = 0;
            for j in 0..other.len {
                if i + j >= MAX_LIMBS {
                    break;
                }
                let prod = self.limbs[i] as u128 * other.limbs[j] as u128
                    + result.limbs[i + j] as u128
                    + carry;
                result.limbs[i + j] = prod as usize;
                carry = prod >> LIMB_BITS;
            }
            if i + other.len < MAX_LIMBS {
                result.limbs[i + other.len] = carry as usize;
            }
        }
        result.trim();
        result
    }

    /// Divide self by a single usize digit. Returns (quotient, remainder).
    pub fn div_single(&self, divisor: usize) -> (Self, usize) {
        if divisor == 0 {
            return (Self::zero(), 0);
        }
        let mut quotient = Self::zero();
        quotient.negative = self.negative;
        quotient.len = self.len;
        let mut rem: u128 = 0;
        for i in (0..self.len).rev() {
            let cur = rem * (1u128 << LIMB_BITS) + self.limbs[i] as u128;
            quotient.limbs[i] = (cur / divisor as u128) as usize;
            rem = cur % divisor as u128;
        }
        quotient.trim();
        (quotient, rem as usize)
    }

    /// Divide self by a single u32 digit. Returns (quotient, remainder).
    /// Kept for backward compatibility.
    pub fn div_u32(&self, divisor: u32) -> (Self, u32) {
        let (q, r) = self.div_single(divisor as usize);
        (q, r as u32)
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
            let (q, r) = self.div_single(other.limbs[0]);
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
        let word_shift = (n / LIMB_BITS) as usize;
        let bit_shift = n % LIMB_BITS;
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
            let mut carry: usize = 0;
            for i in 0..self.len {
                let shifted = ((self.limbs[i] as u128) << bit_shift) | carry as u128;
                if i + word_shift < MAX_LIMBS {
                    result.limbs[i + word_shift] = shifted as usize;
                }
                carry = (shifted >> LIMB_BITS) as usize;
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
        let word_shift = (n / LIMB_BITS) as usize;
        let bit_shift = n % LIMB_BITS;

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
                    result.limbs[i - word_shift] |= self.limbs[i + 1] << (LIMB_BITS - bit_shift);
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
        (self.len as u32 - 1) * LIMB_BITS + (LIMB_BITS - top.leading_zeros())
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
        let word_idx = (shift_bits / LIMB_BITS) as usize;
        let bit_idx = shift_bits % LIMB_BITS;
        if word_idx < MAX_LIMBS {
            x.limbs[word_idx] = 1usize << bit_idx;
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
    let mut carry: u128 = 0;

    for i in 0..max_len {
        let av = if i < a.len { a.limbs[i] as u128 } else { 0 };
        let bv = if i < b.len { b.limbs[i] as u128 } else { 0 };
        let sum = av + bv + carry;
        result.limbs[i] = sum as usize;
        carry = sum >> LIMB_BITS;
    }
    if carry > 0 && max_len < MAX_LIMBS {
        result.limbs[max_len] = carry as usize;
        result.len = max_len + 1;
    } else {
        result.len = max_len;
    }
    result
}

/// Subtract magnitude of b from a (assumes |a| >= |b|, ignoring sign).
fn sub_magnitudes(a: &BigNumBuf, b: &BigNumBuf) -> BigNumBuf {
    let mut result = BigNumBuf::zero();
    let mut borrow: i128 = 0;

    for i in 0..a.len {
        let av = a.limbs[i] as i128;
        let bv = if i < b.len { b.limbs[i] as i128 } else { 0 };
        let diff = av - bv - borrow;
        if diff < 0 {
            result.limbs[i] = (diff + (1i128 << LIMB_BITS)) as usize;
            borrow = 1;
        } else {
            result.limbs[i] = diff as usize;
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
    let mut rem_limbs = [0usize; MAX_LIMBS + 1];
    for i in 0..un.len {
        rem_limbs[i] = un.limbs[i];
    }
    let rem_len = if un.len > u.len { un.len } else { un.len + 1 };

    for j in (0..=m).rev() {
        // Estimate q_hat = (rem[j+n]*b + rem[j+n-1]) / vn[n-1]
        let r_top = if j + n < rem_len { rem_limbs[j + n] as u128 } else { 0 };
        let r_next = if j + n >= 1 { rem_limbs[j + n - 1] as u128 } else { 0 };
        let divisor = vn.limbs[n - 1] as u128;

        let mut q_hat: u128 = if divisor == 0 {
            u128::MAX
        } else {
            let num = (r_top << LIMB_BITS) | r_next;
            num / divisor
        };

        if q_hat > LIMB_MAX {
            q_hat = LIMB_MAX;
        }

        // Refine estimate
        loop {
            let r_hat = (r_top << LIMB_BITS) | r_next;
            let r_hat = r_hat.wrapping_sub(q_hat * divisor);
            if r_hat > LIMB_MAX {
                break; // Will underflow
            }
            if n >= 2 {
                let v_next = vn.limbs[n - 2] as u128;
                let rem_next = if j + n >= 2 { rem_limbs[j + n - 2] as u128 } else { 0 };
                if q_hat * v_next > (r_hat << LIMB_BITS) + rem_next {
                    q_hat -= 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Multiply and subtract: rem[j..j+n] -= q_hat * vn
        let mut borrow: i128 = 0;
        for i in 0..n {
            let prod = q_hat * vn.limbs[i] as u128;
            let val = rem_limbs[j + i] as i128 - (prod as usize as i128) - borrow;
            rem_limbs[j + i] = val as usize;
            borrow = (prod >> LIMB_BITS) as i128 - (val >> LIMB_BITS) as i128;
        }
        if j + n < rem_len {
            let val = rem_limbs[j + n] as i128 - borrow;
            rem_limbs[j + n] = val as usize;
            if val < 0 {
                // q_hat was too large, add back
                q_hat -= 1;
                let mut carry: u128 = 0;
                for i in 0..n {
                    let sum = rem_limbs[j + i] as u128 + vn.limbs[i] as u128 + carry;
                    rem_limbs[j + i] = sum as usize;
                    carry = sum >> LIMB_BITS;
                }
                if j + n < rem_len {
                    rem_limbs[j + n] = rem_limbs[j + n].wrapping_add(carry as usize);
                }
            }
        }

        q.limbs[j] = q_hat as usize;
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
