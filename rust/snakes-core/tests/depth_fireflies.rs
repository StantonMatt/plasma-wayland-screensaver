use snakes_core::visuals::depth::fireflies::{DESCRIPTOR, PARAMETERS, Params};
#[test] fn disabled_contract() {assert!(!std::hint::black_box(DESCRIPTOR.available));assert_eq!(PARAMETERS.len(),5);let _=Params::default();}
