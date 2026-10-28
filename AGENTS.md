- Always consider consistency with SPEC.md and ARCHITECTURE.md before any change
- if you find any discrepancies, unrelated to the current prompt, report them

- all pointer reads should be protected by Copy because to prevent reading opaque types with &mut or a custom Drop.
- the contents of a `&UnsafeCell<T>` must never be read (including through `Cell`) during conversion or validation.
