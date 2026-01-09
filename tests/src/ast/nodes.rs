use inference_ast::nodes::Location;

#[test]
fn test_location_new() {
    let loc = Location::new(0, 10, 1, 0, 1, 10, "hello world".to_string());
    assert_eq!(loc.offset_start, 0);
    assert_eq!(loc.offset_end, 10);
    assert_eq!(loc.start_line, 1);
    assert_eq!(loc.start_column, 0);
    assert_eq!(loc.end_line, 1);
    assert_eq!(loc.end_column, 10);
    assert_eq!(loc.source, "hello world");
}

#[test]
fn test_location_display() {
    let loc = Location::new(5, 15, 2, 3, 2, 13, "test source".to_string());
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
    assert_eq!(loc.source, "");
}

#[test]
fn test_location_clone() {
    let loc = Location::new(0, 5, 1, 0, 1, 5, "test".to_string());
    let cloned = loc.clone();
    assert_eq!(loc, cloned);
}

#[test]
fn test_location_eq() {
    let loc1 = Location::new(0, 5, 1, 0, 1, 5, "test".to_string());
    let loc2 = Location::new(0, 5, 1, 0, 1, 5, "test".to_string());
    assert_eq!(loc1, loc2);
}

#[test]
fn test_location_ne() {
    let loc1 = Location::new(0, 5, 1, 0, 1, 5, "test".to_string());
    let loc2 = Location::new(0, 6, 1, 0, 1, 6, "test2".to_string());
    assert_ne!(loc1, loc2);
}

#[test]
fn test_location_debug() {
    let loc = Location::new(10, 20, 3, 5, 3, 15, "debug test".to_string());
    let debug_str = format!("{loc:?}");
    assert!(debug_str.contains("Location"));
    assert!(debug_str.contains("offset_start: 10"));
}

#[test]
fn test_location_multiline() {
    let loc = Location::new(0, 25, 1, 0, 3, 5, "line1\nline2\nline3".to_string());
    assert_eq!(loc.start_line, 1);
    assert_eq!(loc.end_line, 3);
    assert_eq!(loc.offset_end, 25);
}
