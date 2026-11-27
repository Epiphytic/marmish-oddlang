type Check<T> = std::result::Result<T, &'static str>;
pub(crate) struct Cursor<'a>(pub(crate) &'a [u8]);
impl<'a> Cursor<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Check<&'a [u8]> {
        let (v, rest) = self.0.split_at_checked(n).ok_or("copy_truncated")?;
        self.0 = rest;
        Ok(v)
    }
    pub(crate) fn array<const N: usize>(&mut self) -> Check<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    pub(crate) fn byte(&mut self) -> Check<u8> {
        Ok(self.array::<1>()?[0])
    }
    pub(crate) fn u32(&mut self) -> Check<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    pub(crate) fn u64(&mut self) -> Check<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    pub(crate) fn i64(&mut self) -> Check<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }
    pub(crate) fn bytes(&mut self, limit: usize) -> Check<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > limit {
            return Err("Limit");
        }
        Ok(self.take(len)?.to_vec())
    }
    pub(crate) fn flag(&mut self, error: &'static str) -> Check<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(error),
        }
    }
    pub(crate) fn finish(&self) -> Check<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err("copy_trailing")
        }
    }
}
