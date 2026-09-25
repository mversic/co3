use co3::{
    ReprC,
    ffi,
};
use co3::rust_spec::RustSpec;

#[derive(Clone, Copy, RustSpec, ReprC)]
enum State {
    Disconnected,
    Connected,
}

type StateCType = <State as ReprC>::CType;

#[derive(Clone, Copy, RustSpec, ReprC)]
struct Status {
    state: State,
    optional_state: Option<State>,
}

fn take_status(status: Status) -> bool {
    matches!(status.state, State::Connected)
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "nested_fieldless_enum"]

    fn take_status(status: Status) -> bool;
}

fn main() {}
