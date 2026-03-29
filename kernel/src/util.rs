pub const fn clog2(n: usize) -> usize {
    usize::BITS as usize - (n - 1).leading_zeros() as usize
}
