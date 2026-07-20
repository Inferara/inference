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
//! Supported inputs are local `file:` URIs with an empty or `localhost`
//! authority, in either the authority form (`file:///path`) or the RFC 8089
//! minimal form (`file:/path`); the scheme is matched case-insensitively
//! (`File:`, `FILE:`). The decoded path is lexically normalized — dot segments
//! (`.` / `..`) are removed so one on-disk file interns under one spelling. A
//! non-`file` scheme, a remote authority, a path-form UNC path
//! (`file:////server/share`), a URI carrying a query or fragment, or — on
//! Windows — a bare or drive-relative drive path (`file:///C:`) yields `None`;
//! the caller treats that as "not a document we can analyze" and answers
//! gracefully.

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

/// Decodes a `file:` URI string into its filesystem path string, or `None` when
/// the URI does not name a servable local file.
///
/// The scheme is matched case-insensitively (RFC 3986 §3.1), so `file:`,
/// `File:`, and `FILE:` are equivalent. Both the authority form (`file:///path`,
/// authority empty or `localhost`) and the RFC 8089 minimal form (`file:/path`,
/// no authority) are accepted. The path is percent-decoded, then dot segments
/// (`.` / `..`) are removed lexically so `/a/../b.inf` and `/b.inf` name one
/// document; on a Windows host the `file:///C:/…` drive form is canonicalized
/// (leading slash dropped, drive letter upper-cased).
///
/// `None` is returned for a non-`file` scheme, a remote authority, a path-form
/// UNC path (empty authority with a `//` path, e.g. `file:////server/share`,
/// which is network I/O on Windows), a query or fragment, or — on Windows — a
/// bare or drive-relative drive path (`file:///C:`, `file:///c:name`).
///
/// # Lexical only
///
/// Dot-segment removal is purely textual: a `..` that would cross a symlink is
/// resolved by name, not by following the link. Callers needing symlink-correct
/// identity must canonicalize against the filesystem.
fn file_uri_to_path(uri: &str, windows: bool) -> Option<String> {
    let after_scheme = strip_file_scheme(uri)?;
    let path = isolate_path(after_scheme)?;
    if path.starts_with("//") {
        // A `//`-prefixed path is a path-form UNC (empty authority, e.g.
        // `file:////server/share/x`): a network path that triggers SMB I/O on
        // Windows. Rejected like the remote-authority case. This is a cheap
        // early exit; the post-decode check below closes the encoded variant.
        return None;
    }
    if has_query_or_fragment(path) {
        return None; // Not a plain document URI; the caller handles it gracefully.
    }
    let decoded = percent_decode(path)?;
    if decoded.starts_with("//") {
        // Percent-encoded leading slashes (`file:///%2F%2Fserver/share`) slip
        // past the raw check above, so the decoded form is re-checked: a UNC
        // path is rejected regardless of how its slashes were spelled.
        return None;
    }
    normalize_path(decoded, windows)
}

/// Strips a case-insensitive `file:` scheme, returning the URI body after the
/// colon, or `None` for any other scheme.
///
/// The scheme name is compared case-insensitively per RFC 3986 §3.1, so `file:`,
/// `File:`, and `FILE:` all name the file scheme.
fn strip_file_scheme(uri: &str) -> Option<&str> {
    let colon = uri.find(':')?;
    uri[..colon]
        .eq_ignore_ascii_case("file")
        .then_some(&uri[colon + 1..])
}

/// Isolates the path component of a `file:` URI body (everything after the
/// scheme's colon), accepting both URI forms and rejecting a remote authority.
///
/// * Authority form (`//authority/path`): the authority — empty or `localhost` —
///   is stripped and the leading-slash path returned; any other authority names a
///   remote host this server does not serve.
/// * RFC 8089 minimal form (`/path`, no `//`): returned unchanged.
///
/// Returns `None` for a remote authority or a body naming no absolute path.
fn isolate_path(body: &str) -> Option<&str> {
    if let Some(after_slashes) = body.strip_prefix("//") {
        let path_start = after_slashes.find('/')?;
        let authority = &after_slashes[..path_start];
        if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
            return None; // A remote/UNC authority is out of scope for v1.
        }
        Some(&after_slashes[path_start..])
    } else if body.starts_with('/') {
        Some(body)
    } else {
        None // Not an absolute-path file URI (e.g. `file:` with no path).
    }
}

/// Lexically normalizes a decoded absolute path for the host, or `None` when it
/// does not name an absolute path.
///
/// On Windows a `/X:/…` drive path has its leading slash dropped and its drive
/// letter upper-cased, and a bare (`/X:`) or drive-relative (`/X:name`) form —
/// which resolves against a per-drive working directory rather than a fixed
/// location — is rejected; the drive-anchored remainder is dot-normalized. Every
/// other absolute path (including a POSIX `/X:` directory literally named `X:`)
/// is dot-normalized with its leading slash intact.
fn normalize_path(decoded: String, windows: bool) -> Option<String> {
    let drive = if windows { split_drive(&decoded) } else { None };
    let Some((letter, rest)) = drive else {
        return Some(remove_dot_segments(&decoded));
    };
    // `rest` is everything after `X:`; it must begin with `/` for the path to be
    // absolute. A bare `X:` (empty rest) or drive-relative `X:name` (rest not
    // `/`-led) names no fixed location, so it is rejected.
    if !rest.starts_with('/') {
        return None;
    }
    let letter = letter.to_ascii_uppercase();
    Some(format!("{letter}:{}", remove_dot_segments(rest)))
}

/// Splits a decoded `/X:…` Windows drive path into its drive letter and the
/// remainder after `X:`, or `None` when `path` is not a drive URI.
fn split_drive(path: &str) -> Option<(char, &str)> {
    let bytes = path.as_bytes();
    let is_drive_uri = bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':';
    is_drive_uri.then(|| (bytes[1] as char, &path[3..]))
}

/// Lexically removes `.`, `..`, and empty segments from an absolute path,
/// matching RFC 3986 §5.2.4 remove_dot_segments.
///
/// The input begins with `/`; the result is rebuilt from its surviving segments,
/// always single-slash-joined and absolute (an all-dots path collapses to `/`). A
/// leading `..` at the root is dropped, so the result never escapes above the
/// root and never begins with `//`.
fn remove_dot_segments(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            normal => segments.push(normal),
        }
    }
    let mut normalized = String::with_capacity(path.len());
    for segment in segments {
        normalized.push('/');
        normalized.push_str(segment);
    }
    if normalized.is_empty() {
        normalized.push('/');
    }
    normalized
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
        // VS Code sends the drive colon percent-encoded and lowercased; on
        // Windows both spellings must decode to the one std-canonical (uppercase)
        // path, so the overlay and the analysis memo key them as one document.
        let lower = file_uri_to_path("file:///c%3A/Users/x/main.inf", WINDOWS);
        let upper = file_uri_to_path("file:///C:/Users/x/main.inf", WINDOWS);
        assert_eq!(lower.as_deref(), Some("C:/Users/x/main.inf"));
        assert_eq!(lower, upper, "the two drive spellings name one document");
    }

    #[test]
    fn posix_drive_shape_stays_absolute() {
        // On POSIX `/c:/…` is a genuine absolute path (a directory literally
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
        // A document URI carries neither a query nor a fragment; both are
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
        // On POSIX a backslash is an ordinary byte; from_path percent-encodes
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

    #[test]
    fn dot_segments_are_removed() {
        // `/a/../b.inf` and `/b.inf` must intern as one document.
        let indirect = file_uri_to_path("file:///a/../b.inf", POSIX);
        let direct = file_uri_to_path("file:///b.inf", POSIX);
        assert_eq!(indirect.as_deref(), Some("/b.inf"));
        assert_eq!(indirect, direct, "the two spellings name one document");
    }

    #[test]
    fn dot_segment_variants_all_normalize() {
        // Interior `.`, trailing `.`, leading `..` at root, consecutive `..`.
        for (uri, expected) in [
            ("file:///a/./b.inf", "/a/b.inf"),
            ("file:///a/b/.", "/a/b"),
            ("file:///a/b/./", "/a/b"),
            ("file:///../b.inf", "/b.inf"),
            ("file:///a/./../b.inf", "/b.inf"),
            ("file:///a/b/../../c.inf", "/c.inf"),
            ("file:///a/b/../c/../d.inf", "/a/d.inf"),
            // A `..` cannot escape the root; the extra one is dropped.
            ("file:///../../x.inf", "/x.inf"),
            // An all-dots path collapses to the root.
            ("file:///a/..", "/"),
        ] {
            assert_eq!(
                file_uri_to_path(uri, POSIX).as_deref(),
                Some(expected),
                "normalizing {uri}"
            );
        }
    }

    #[test]
    fn dot_segments_normalize_on_windows_drive() {
        // The drive path is preserved while interior dot segments are removed.
        assert_eq!(
            file_uri_to_path("file:///C:/a/../b.inf", WINDOWS).as_deref(),
            Some("C:/b.inf")
        );
        assert_eq!(
            file_uri_to_path("file:///C:/a/./b.inf", WINDOWS).as_deref(),
            Some("C:/a/b.inf")
        );
    }

    #[test]
    fn dot_segment_normalization_is_direction_symmetric() {
        // Whichever spelling arrives, both decode to the same normalized path.
        let a = file_uri_to_path("file:///x/../y/main.inf", POSIX);
        let b = file_uri_to_path("file:///y/./main.inf", POSIX);
        let c = file_uri_to_path("file:///y/main.inf", POSIX);
        assert_eq!(a.as_deref(), Some("/y/main.inf"));
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn path_form_unc_is_rejected() {
        // An empty authority with a `//` path is a UNC path (SMB I/O on
        // Windows), rejected like a remote authority on every host.
        assert_eq!(file_uri_to_path("file:////server/share/x.inf", POSIX), None);
        assert_eq!(
            file_uri_to_path("file:////server/share/x.inf", WINDOWS),
            None
        );
        // Even more leading slashes stay rejected.
        assert_eq!(file_uri_to_path("file://///server/x.inf", POSIX), None);
    }

    #[test]
    fn percent_encoded_path_form_unc_is_rejected() {
        // Percent-encoded leading slashes evade the raw `//` check but decode to
        // a UNC path, so the decoded form is re-checked and rejected on every
        // host. Without it the URI would silently normalize to a local path.
        assert_eq!(
            file_uri_to_path("file:///%2F%2Fserver/share/x.inf", POSIX),
            None
        );
        assert_eq!(
            file_uri_to_path("file:///%2F%2Fserver/share/x.inf", WINDOWS),
            None
        );
        // A mixed spelling (one raw, one encoded slash) is rejected too.
        assert_eq!(file_uri_to_path("file:///%2F/server/x.inf", POSIX), None);
    }

    #[test]
    fn single_encoded_slash_inside_a_path_decodes() {
        // A lone interior `%2F` is an ordinary encoded separator, not a UNC
        // prefix, so it decodes to a normal absolute path rather than being
        // rejected by the post-decode `//` guard.
        assert_eq!(
            file_uri_to_path("file:///home/a%2Fb.inf", POSIX).as_deref(),
            Some("/home/a/b.inf")
        );
    }

    #[test]
    fn scheme_is_matched_case_insensitively() {
        // RFC 3986 §3.1 — the scheme is case-insensitive.
        let canonical = file_uri_to_path("file:///home/a.inf", POSIX);
        assert_eq!(canonical.as_deref(), Some("/home/a.inf"));
        for spelling in ["File:///home/a.inf", "FILE:///home/a.inf", "FiLe:///home/a.inf"] {
            assert_eq!(
                file_uri_to_path(spelling, POSIX),
                canonical,
                "scheme spelling {spelling} must name one document"
            );
        }
        // A non-`file` scheme is still rejected regardless of case.
        assert_eq!(file_uri_to_path("HTTP://example.com/x.inf", POSIX), None);
    }

    #[test]
    fn minimal_single_slash_form_is_accepted() {
        // RFC 8089 `file:/path` (no authority) is spec-valid and trivially
        // normalized to the same path as the authority form.
        let minimal = file_uri_to_path("file:/home/user/main.inf", POSIX);
        let authority = file_uri_to_path("file:///home/user/main.inf", POSIX);
        assert_eq!(minimal.as_deref(), Some("/home/user/main.inf"));
        assert_eq!(minimal, authority, "both forms name one document");
        // The minimal form also carries a Windows drive path.
        assert_eq!(
            file_uri_to_path("file:/C:/Users/x/main.inf", WINDOWS).as_deref(),
            Some("C:/Users/x/main.inf")
        );
        // The minimal form is dot-normalized too.
        assert_eq!(
            file_uri_to_path("file:/a/../b.inf", POSIX).as_deref(),
            Some("/b.inf")
        );
    }

    #[test]
    fn bare_and_relative_drive_uris_are_rejected_on_windows() {
        // A drive prefix must yield an absolute path. A bare `C:` or a
        // drive-relative `C:name` resolve against a per-drive working directory,
        // so they are not documents this server can serve.
        assert_eq!(file_uri_to_path("file:///C:", WINDOWS), None);
        assert_eq!(file_uri_to_path("file:///c:", WINDOWS), None);
        assert_eq!(file_uri_to_path("file:///c%3A", WINDOWS), None);
        assert_eq!(file_uri_to_path("file:///c:name", WINDOWS), None);
        assert_eq!(file_uri_to_path("file:///C:name/sub", WINDOWS), None);
        // The absolute drive-root and drive paths remain accepted.
        assert_eq!(file_uri_to_path("file:///C:/", WINDOWS).as_deref(), Some("C:/"));
        assert_eq!(
            file_uri_to_path("file:///C:/x", WINDOWS).as_deref(),
            Some("C:/x")
        );
    }

    #[test]
    fn bare_drive_shape_stays_absolute_on_posix() {
        // On POSIX `/C:` is a genuine absolute path (a directory named `C:`), not
        // a Windows drive, so it is accepted and left untouched.
        assert_eq!(
            file_uri_to_path("file:///C:", POSIX).as_deref(),
            Some("/C:")
        );
        assert_eq!(
            file_uri_to_path("file:///c:name", POSIX).as_deref(),
            Some("/c:name")
        );
    }

    #[test]
    fn body_without_a_path_is_rejected() {
        // `file:` bodies that name no absolute path are rejected on every host.
        assert_eq!(file_uri_to_path("file:", POSIX), None);
        assert_eq!(file_uri_to_path("file://", POSIX), None);
        assert_eq!(file_uri_to_path("file://server", POSIX), None);
        assert_eq!(file_uri_to_path("file:relative/main.inf", POSIX), None);
    }

    #[test]
    fn normalized_uri_round_trips_through_a_path() {
        // A normalized path re-encodes to the canonical (authority) URI form and
        // decodes back unchanged.
        for path in ["/home/user/main.inf", "/a/b/c.inf"] {
            let uri = path_to_file_uri(path, POSIX).expect("uri");
            assert_eq!(file_uri_to_path(&uri, POSIX).as_deref(), Some(path));
        }
    }
}
