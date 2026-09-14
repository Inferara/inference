//! The `contractenvmetav0` custom section: the one thing a contract must carry
//! that cannot be derived from the module being rewritten.
//!
//! Its absence is refused at upload — measured as `Error(WasmVm, InvalidInput)`,
//! `contract missing metadata section` — and so is a declared protocol newer
//! than the ledger's.

use wasm_encoder::CustomSection;

/// The environment protocol this toolchain declares.
///
/// A contract declaring protocol *p* uploads to any host whose ledger protocol
/// is at least *p*, provided the pre-release number is 0. Declaring the newest
/// protocol would therefore make an artifact un-uploadable to every network that
/// has not yet caught up, while declaring 20 — the first protocol with Soroban
/// in it — uploads to all of them. Measured accepted by a protocol-28 host;
/// protocol 99 is refused with `contract protocol number is newer than host`.
///
/// **The rule this constant must follow once host imports are admitted at this
/// target (#324): it becomes the maximum introduction protocol over every host
/// function the contract imports.** A contract that imports a function
/// introduced in protocol 22 and declares 20 is claiming to run on a host where
/// that import does not resolve. A host binding is refused at this target
/// today, so a contract imports nothing and there is nothing to bound, which is
/// the only reason the floor is usable.
pub const STELLAR_ENV_PROTOCOL: u32 = 20;

/// The pre-release number this toolchain declares.
///
/// The host's version check has exactly one extra clause below the ledger
/// protocol: the pre-release must be 0. Anything else is a pre-release build of
/// the environment, which a released host will not run.
pub const STELLAR_ENV_PRE_RELEASE: u32 = 0;

/// Name of the custom section carrying the declared environment version.
pub(crate) const META_SECTION_NAME: &str = "contractenvmetav0";

/// `SC_ENV_META_KIND_INTERFACE_VERSION`, the only `SCEnvMetaEntry` kind a
/// contract needs: the union discriminant that says the twelve bytes are an
/// interface version rather than anything else.
const SC_ENV_META_KIND_INTERFACE_VERSION: u32 = 0;

/// The twelve-byte XDR payload: the entry kind, then the protocol, then the
/// pre-release, each a big-endian `uint32`.
pub(crate) fn metadata_payload(protocol: u32, pre_release: u32) -> [u8; 12] {
    let mut payload = [0u8; 12];
    payload[0..4].copy_from_slice(&SC_ENV_META_KIND_INTERFACE_VERSION.to_be_bytes());
    payload[4..8].copy_from_slice(&protocol.to_be_bytes());
    payload[8..12].copy_from_slice(&pre_release.to_be_bytes());
    payload
}

/// The whole section, ready to append to a module.
pub(crate) fn metadata_section(payload: &[u8]) -> CustomSection<'_> {
    CustomSection {
        name: META_SECTION_NAME.into(),
        data: payload.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_encoder::Module;

    #[test]
    fn the_payload_is_the_twelve_measured_bytes() {
        assert_eq!(
            metadata_payload(STELLAR_ENV_PROTOCOL, STELLAR_ENV_PRE_RELEASE),
            [0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0]
        );
    }

    /// The section as a whole, header included, is the 32-byte run
    /// `MEASURED_ABI.md` pins as the reference contract's tail: custom-section
    /// id, a payload size of 30, a name length of 17, the name, and the payload.
    #[test]
    fn the_section_is_the_thirty_two_measured_bytes() {
        let payload = metadata_payload(STELLAR_ENV_PROTOCOL, STELLAR_ENV_PRE_RELEASE);
        let mut module = Module::new();
        module.section(&metadata_section(&payload));
        let bytes = module.finish();
        let section = &bytes[8..];

        assert_eq!(
            section,
            &[
                0x00, 0x1e, 0x11, b'c', b'o', b'n', b't', b'r', b'a', b'c', b't', b'e', b'n', b'v',
                b'm', b'e', b't', b'a', b'v', b'0', 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0
            ]
        );
        assert_eq!(section.len(), 32);
    }

    /// A different protocol moves exactly the four protocol bytes, which is what
    /// makes the constant the only thing standing between this toolchain and a
    /// network it cannot upload to.
    #[test]
    fn the_declared_protocol_is_the_only_thing_a_protocol_change_moves() {
        let twenty = metadata_payload(20, 0);
        let twenty_eight = metadata_payload(28, 0);
        assert_eq!(twenty[0..4], twenty_eight[0..4]);
        assert_eq!(twenty[8..12], twenty_eight[8..12]);
        assert_eq!(twenty[4..8], [0, 0, 0, 20]);
        assert_eq!(twenty_eight[4..8], [0, 0, 0, 28]);
    }
}
