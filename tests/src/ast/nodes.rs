use inference_ast::nodes::Location;

use always_assert::always;
#[test]
fn test_location_new() {
    let loc = Location::new(0, 10, 1, 0, 1, 10);
    assert_eq!(loc.offset_start, 0);
    assert_eq!(loc.offset_end, 10);
    assert_eq!(loc.start_line, 1);
    assert_eq!(loc.start_column, 0);
    assert_eq!(loc.end_line, 1);
    assert_eq!(loc.end_column, 10);
}

#[test]
fn test_location_display() {
    let loc = Location::new(5, 15, 2, 3, 2, 13);
    let display = format!("{loc}");
    assert_eq!(display, "2:3");
}

#[test]
fn test_location_default() {
    let loc = Location::default();
    assert_eq!(loc.offset_start, 0);
    assert_eq!(loc.offset_end, 0);
    assert_eq!(loc.start_line, 0);
    assert_eq!(loc.start_column, 0);
    assert_eq!(loc.end_line, 0);
    assert_eq!(loc.end_column, 0);
}

#[test]
#[allow(clippy::clone_on_copy)]
fn test_location_clone() {
    // Explicitly test Clone trait works (Copy provides Clone automatically)
    let loc = Location::new(0, 5, 1, 0, 1, 5);
    let cloned = loc.clone();
    assert_eq!(loc, cloned);
}

#[test]
fn test_location_eq() {
    let loc1 = Location::new(0, 5, 1, 0, 1, 5);
    let loc2 = Location::new(0, 5, 1, 0, 1, 5);
    assert_eq!(loc1, loc2);
}

#[test]
fn test_location_ne() {
    let loc1 = Location::new(0, 5, 1, 0, 1, 5);
    let loc2 = Location::new(0, 6, 1, 0, 1, 6);
    assert_ne!(loc1, loc2);
}

#[test]
fn test_location_debug() {
    let loc = Location::new(10, 20, 3, 5, 3, 15);
    let debug_str = format!("{loc:?}");
    always!(debug_str.contains("Location"));
    always!(debug_str.contains("offset_start: 10"));
}

#[test]
fn test_location_multiline() {
    let loc = Location::new(0, 25, 1, 0, 3, 5);
    assert_eq!(loc.start_line, 1);
    assert_eq!(loc.end_line, 3);
    assert_eq!(loc.offset_end, 25);
}

#[test]
fn test_location_copy() {
    let loc = Location::new(0, 5, 1, 0, 1, 5);
    let copied = loc;
    assert_eq!(loc, copied);
}

#[test]
fn test_location_copy_allows_multiple_uses() {
    let loc = Location::new(0, 5, 1, 0, 1, 5);
    let _copy1 = loc;
    let _copy2 = loc;
    let _copy3 = loc;
    assert_eq!(loc.offset_start, 0);
}

#[test]
fn test_location_copy_in_function_arguments() {
    fn use_location(loc: Location) -> u32 {
        loc.offset_end
    }
    let loc = Location::new(0, 42, 1, 0, 1, 42);
    let result1 = use_location(loc);
    let result2 = use_location(loc);
    assert_eq!(result1, 42);
    assert_eq!(result2, 42);
}

#[test]
fn test_location_edge_case_zero_values() {
    let loc = Location::new(0, 0, 0, 0, 0, 0);
    assert_eq!(loc.offset_start, 0);
    assert_eq!(loc.offset_end, 0);
    let copied = loc;
    assert_eq!(copied, loc);
}

#[test]
fn test_location_edge_case_max_values() {
    let max = u32::MAX;
    let loc = Location::new(max, max, max, max, max, max);
    assert_eq!(loc.offset_start, max);
    let copied = loc;
    assert_eq!(copied.offset_end, max);
}

#[test]
fn test_location_display_zero_values() {
    let loc = Location::new(0, 0, 0, 0, 0, 0);
    assert_eq!(format!("{loc}"), "0:0");
}
