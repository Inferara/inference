//! Conversions between `file://` URIs and filesystem paths.
//!
//! `lsp-types` models a URI with `fluent-uri`, which offers no path helpers, so
//! the mapping is done here as pure string manipulation and kept in one place —
//! it is the only spot in the server that reasons about percent-encoding and
//! Windows drive letters. The two public wrappers ([`to_path`] / [`from_path`])
//! bridge to `std` path and `lsp_types::Uri` for the host they run on.
//!
//! # Drive letters are host-shaped
//!
//! The `/X:/…` form is a Windows drive path *only on Windows*. On a POSIX host it
//! is a genuine absolute path whose first component happens to be a directory
//! named `X:`. The string core therefore takes an explicit `windows` flag
//! (`cfg!(windows)` in the public wrappers) instead of pattern-matching the shape
//! host-blindly: on Windows the leading slash is dropped and the drive letter is
//! upper-cased so `file:///c%3A/…` and `file:///C:/…` name one document (the std
//! canonical spelling); on POSIX `/c:/…` maps to itself and a backslash is an
//! ordinary filename byte. The flag is a parameter so both behaviours stay
//! testable from either host.
//!
//! Supported inputs are local `file://` URIs with an empty or `localhost`
//! authority. A non-`file` scheme, a remote authority, or a URI carrying a query
//! or fragment yields `None`; the caller treats that as "not a document we can
//! analyze" and answers gracefully.

use std::path::{Path, PathBuf};

use lsp_types::Uri;

/// The filesystem path a `file://` URI names, or `None` when it is not a local
/// file URI this server can serve.
#[must_use = "a URI that cannot be mapped to a path must be handled, not ignored"]
pub(crate) fn to_path(uri: &Uri) -> Option<PathBuf> {
    file_uri_to_path(uri.as_str(), cfg!(windows)).map(PathBuf::from)
}

/// The `file://` URI naming `path`, or `None` when `path` is not absolute or is
/// not valid UTF-8.
#[must_use = "a path that cannot be mapped to a URI must be handled, not ignored"]
pub(crate) fn from_path(path: &Path) -> Option<Uri> {
    let uri = path_to_file_uri(path.to_str()?, cfg!(windows))?;
    uri.parse().ok()
}

/// Decodes a `file://` URI string into its filesystem path string.
///
/// The path is percent-decoded; on a Windows host the `file:///C:/…` drive form
/// is canonicalized (leading slash dropped, drive letter upper-cased). A URI that
/// carries a query or fragment is rejected — see [`has_query_or_fragment`].
fn file_uri_to_path(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // The authority ends at the first slash, which also begins the absolute path.
    let path_start = rest.find('/')?;
    let authority = &rest[..path_start];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None; // A remote/UNC authority is out of scope for v1.
    }
    let path = &rest[path_start..];
    if has_query_or_fragment(path) {
        return None; // Not a plain document URI; the caller handles it gracefully.
    }
    let decoded = percent_decode(path)?;
    Some(canonicalize_drive(decoded, windows))
}

/// Encodes a filesystem path string as a `file://` URI string.
///
/// On Windows, backslashes are normalized to forward slashes and a drive-letter
/// path gains the leading slash the URI form requires with an upper-cased drive
/// letter (`c:\x` → `file:///C:/x`). On POSIX only an absolute path has a URI, and
/// a backslash is percent-encoded like any other non-path byte.
fn path_to_file_uri(path: &str, windows: bool) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let absolute = if windows {
        let mut normalized = path.replace('\\', "/");
        if starts_with_drive(&normalized) {
            normalized[..1].make_ascii_uppercase();
            format!("/{normalized}")
        } else if normalized.starts_with('/') {
            normalized
        } else {
            return None; // Only absolute paths have a well-defined file URI.
        }
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        return None; // On POSIX a drive-shaped path is relative, so it has no URI.
    };
    Some(format!("file://{}", percent_encode(&absolute)))
}

/// Whether the URI path component begins a query (`?`) or fragment (`#`).
///
/// These delimiters cannot be literal path bytes in a valid URI — a literal `?`
/// or `#` in a filename arrives percent-encoded (`%3F` / `%23`) and never reaches
/// this raw scan. A file URI carrying either is not a document this server can
/// serve, so it is rejected rather than decoded into a path with the query or
/// fragment fused onto the filename.
fn has_query_or_fragment(path: &str) -> bool {
    path.bytes().any(|byte| byte == b'?' || byte == b'#')
}

/// Whether `path` begins with a `X:` drive prefix.
fn starts_with_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// On Windows, canonicalizes a decoded `/X:/…` drive path to `X:/…` with an
/// upper-cased drive letter. On POSIX the same shape is a genuine absolute path
/// and is returned unchanged.
fn canonicalize_drive(path: String, windows: bool) -> String {
    if !windows {
        return path;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        let mut stripped = path[1..].to_owned();
        stripped[..1].make_ascii_uppercase();
        stripped
    } else {
        path
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

/// Percent-encodes every byte outside the unreserved set (plus the path
/// characters `/` and `:`), so the result is a valid URI path that round-trips.
fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for &byte in input.as_bytes() {
        if is_unreserved_path_byte(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX_DIGITS[(byte >> 4) as usize] as char);
            encoded.push(HEX_DIGITS[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

/// Percent-decodes a URI component, returning `None` on a truncated or non-UTF-8
/// escape sequence. Bytes are accumulated first, then decoded as UTF-8, so a
/// multi-byte character split across several `%XX` escapes decodes correctly.
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn is_unreserved_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use lsp_types::Uri;

    use super::{file_uri_to_path, from_path, path_to_file_uri, to_path};

    // The string core is exercised for both hosts explicitly, so a POSIX build
    // still covers the Windows drive-letter behaviour and vice versa.
    const POSIX: bool = false;
    const WINDOWS: bool = true;

    #[test]
    fn posix_absolute_path_round_trips() {
        let path = "/home/user/main.inf";
        let uri = path_to_file_uri(path, POSIX).expect("an absolute posix path has a uri");
        assert_eq!(uri, "file:///home/user/main.inf");
        assert_eq!(file_uri_to_path(&uri, POSIX).as_deref(), Some(path));
    }

    #[test]
    fn spaces_are_percent_encoded_and_decoded() {
        let path = "/home/user/a b.inf";
        let uri = path_to_file_uri(path, POSIX).expect("uri");
        assert_eq!(uri, "file:///home/user/a%20b.inf");
        assert_eq!(file_uri_to_path(&uri, POSIX).as_deref(), Some(path));
    }

    #[test]
    fn non_ascii_is_encoded_as_utf8_bytes() {
        // `naïve` — the `ï` is two UTF-8 bytes 0xC3 0xAF, each escaped.
        let path = "/home/na\u{ef}ve.inf";
        let uri = path_to_file_uri(path, POSIX).expect("uri");
        assert_eq!(uri, "file:///home/na%C3%AFve.inf");
        assert_eq!(file_uri_to_path(&uri, POSIX).as_deref(), Some(path));
    }

    #[test]
    fn windows_drive_forward_slash_round_trips() {
        let path = "C:/Users/x/main.inf";
        let uri = path_to_file_uri(path, WINDOWS).expect("uri");
        assert_eq!(uri, "file:///C:/Users/x/main.inf");
        assert_eq!(file_uri_to_path(&uri, WINDOWS).as_deref(), Some(path));
    }

    #[test]
    fn windows_backslashes_normalize_to_forward_slashes() {
        let uri = path_to_file_uri("C:\\Users\\x\\main.inf", WINDOWS).expect("uri");
        assert_eq!(uri, "file:///C:/Users/x/main.inf");
    }

    #[test]
    fn windows_drive_case_is_canonicalized_to_uppercase() {
        // [15]: VS Code sends the drive colon percent-encoded and lowercased; on
        // Windows both spellings must decode to the one std-canonical (uppercase)
        // path, so the overlay and the analysis memo key them as one document.
        let lower = file_uri_to_path("file:///c%3A/Users/x/main.inf", WINDOWS);
        let upper = file_uri_to_path("file:///C:/Users/x/main.inf", WINDOWS);
        assert_eq!(lower.as_deref(), Some("C:/Users/x/main.inf"));
        assert_eq!(lower, upper, "the two drive spellings name one document");
    }

    #[test]
    fn posix_drive_shape_stays_absolute() {
        // [18]: on POSIX `/c:/…` is a genuine absolute path (a directory literally
        // named `c:`), never a Windows drive — it must not lose its leading slash
        // nor have its case folded, and it must round-trip.
        let path = file_uri_to_path("file:///c%3A/proj/main.inf", POSIX);
        assert_eq!(path.as_deref(), Some("/c:/proj/main.inf"));
        assert!(path.as_deref().unwrap().starts_with('/'), "still absolute");
        let uri = path_to_file_uri("/c:/proj/main.inf", POSIX).expect("uri");
        assert_eq!(
            file_uri_to_path(&uri, POSIX).as_deref(),
            Some("/c:/proj/main.inf")
        );
        // A drive-shaped path is relative on POSIX, so it has no file URI.
        assert_eq!(path_to_file_uri("c:/x", POSIX), None);
    }

    #[test]
    fn query_and_fragment_uris_are_rejected() {
        // [16]: a document URI carries neither a query nor a fragment; both are
        // rejected rather than fused onto the filename.
        assert_eq!(file_uri_to_path("file:///a.inf?x=1#f", POSIX), None);
        assert_eq!(file_uri_to_path("file:///a.inf?ver=1", POSIX), None);
        assert_eq!(file_uri_to_path("file:///a.inf#L5", POSIX), None);
        // A percent-encoded `?` (%3F) is a literal filename byte, not a query.
        assert_eq!(
            file_uri_to_path("file:///a%3Fb.inf", POSIX).as_deref(),
            Some("/a?b.inf")
        );
    }

    #[test]
    fn posix_backslash_is_a_literal_filename_byte() {
        // [17]: on POSIX a backslash is an ordinary byte; from_path percent-encodes
        // it (%5C) rather than rewriting it to a slash, so the round trip is exact.
        let path = "/home/a\\b.inf";
        let uri = path_to_file_uri(path, POSIX).expect("uri");
        assert_eq!(uri, "file:///home/a%5Cb.inf");
        assert_eq!(file_uri_to_path(&uri, POSIX).as_deref(), Some(path));
    }

    #[test]
    fn localhost_authority_is_accepted() {
        assert_eq!(
            file_uri_to_path("file://localhost/home/user/main.inf", POSIX).as_deref(),
            Some("/home/user/main.inf")
        );
    }

    #[test]
    fn non_file_schemes_are_rejected() {
        assert_eq!(file_uri_to_path("http://example.com/x.inf", POSIX), None);
        assert_eq!(file_uri_to_path("untitled:Untitled-1", POSIX), None);
        assert_eq!(file_uri_to_path("inmemory://model/1", POSIX), None);
    }

    #[test]
    fn remote_authority_is_rejected() {
        assert_eq!(
            file_uri_to_path("file://server/share/main.inf", POSIX),
            None
        );
    }

    #[test]
    fn relative_path_has_no_file_uri() {
        assert_eq!(path_to_file_uri("relative/main.inf", POSIX), None);
        assert_eq!(path_to_file_uri("", POSIX), None);
    }

    #[test]
    fn truncated_escape_is_rejected() {
        assert_eq!(file_uri_to_path("file:///home/a%2", POSIX), None);
        assert_eq!(file_uri_to_path("file:///home/a%zz", POSIX), None);
    }

    #[test]
    fn literal_percent_round_trips() {
        let path = "/home/100%done/main.inf";
        let uri = path_to_file_uri(path, POSIX).expect("uri");
        assert_eq!(uri, "file:///home/100%25done/main.inf");
        assert_eq!(file_uri_to_path(&uri, POSIX).as_deref(), Some(path));
    }

    #[test]
    fn uri_wrappers_round_trip_through_lsp_type() {
        let path = PathBuf::from("/home/user/main.inf");
        let uri = from_path(&path).expect("a uri for an absolute path");
        assert_eq!(uri.as_str(), "file:///home/user/main.inf");
        assert_eq!(to_path(&uri), Some(path));
    }

    #[test]
    fn to_path_rejects_a_non_file_uri() {
        let uri = Uri::from_str("untitled:Untitled-1").expect("a syntactically valid uri");
        assert_eq!(to_path(&uri), None);
    }
}
