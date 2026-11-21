use snakes_core::visuals::depth::{
    MirroredPolicy,
    matrix::{DESCRIPTOR, PARAMETERS},
};
#[test]
fn digital_rain_contract() {
    assert_eq!(
        (DESCRIPTOR.id, DESCRIPTOR.key, DESCRIPTOR.rate),
        (6, "matrix", 30)
    );
    assert_eq!(DESCRIPTOR.mirrored, MirroredPolicy::PerOutputLocal);
    assert_eq!(PARAMETERS.len(), 5);
    assert_eq!(
        snakes_core::visuals::depth::ffi::visuals_depth_available(DESCRIPTOR.id),
        1
    );
}
