extern crate sqlite;

pub trait TextField {
    fn read_text(&self, i: usize) -> Option<String>;
}

impl<'l> TextField for sqlite::Statement<'l> {
    fn read_text(&self, i: usize) -> Option<String> {
        match self.read::<Vec<u8>>(i) {
            Err(_) => None,
            Ok(x) => String::from_utf8(x).ok()
        }
    }
}

pub trait UnsignedField {
    fn read_u64(&self, i: usize) -> Result<u64, sqlite::Error>;
}

impl<'l> UnsignedField for sqlite::Statement<'l> {
    fn read_u64(&self, i: usize) -> Result<u64, sqlite::Error> {
        self.read::<i64>(i).map(|i| i as u64)
    }
}
