// Call the exported symbol with the literal signature, regardless of its spelling in the fixture.
co3::ffi! {
    #![unsafe(extern("C"))]
    pub fn fill(buf: &mut [u8]);
}

pub fn check(expected: [u8; 3]) {
    let mut buf = [9, 8, 7];
    fill(&mut buf);
    assert_eq!(buf, expected);
}
