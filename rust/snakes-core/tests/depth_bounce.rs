use snakes_core::visuals::depth::bounce::{DESCRIPTOR, PARAMETERS, Params};
#[test] fn disabled_contract() {assert!(!std::hint::black_box(DESCRIPTOR.available));assert_eq!(PARAMETERS.len(),6);let _=Params::default();}
