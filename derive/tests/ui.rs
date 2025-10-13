use trybuild::TestCases;

#[test]
fn ui() {
    let test_cases = TestCases::new();
    test_cases.pass("tests/ui_pass/[!getset_]*.rs");
    test_cases.compile_fail("tests/ui_fail/[!getset_]*.rs");

    #[cfg(feature = "getset")]
    {
        test_cases.pass("tests/ui_pass/getset_*.rs");
        test_cases.compile_fail("tests/ui_fail/getset_*.rs");
    }
}
