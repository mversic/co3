#[co3::shared]
pub(crate) trait Custom {
    fn inc(self) -> Self;
    fn dec(self) -> Self;
    fn touch(&mut self);
    fn add_and_get(&mut self, inc: u8) -> u8;
    fn seeded(id: u8) -> Self;
}

#[co3::shared]
pub(crate) trait ExtraCustom {
    fn bump2(self) -> Self;
}

//mod handles;
mod niche_value;
mod zst;
