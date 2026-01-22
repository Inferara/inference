#[cfg(test)]
mod tests {
    use inf_wast::{
        Wat,
        parser::{self, ParseBuffer},
    };

    #[test]
    fn parse_vanilla_i32_set_module() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (result i32)
                (local $var i32)
                (local.set $var (i32.const 10))
            ))
        "#,
        )?;
        parser::parse::<Wat>(&input)?;
        Ok(())
    }

    #[test]
    fn parse_uzumaki_i32_set_module() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func
                (local $var i32)
                (local.set $var (i32.uzumaki))
            ))
        "#,
        )?;
        let mut wat = parser::parse::<Wat>(&input)?;
        let wasm = wat.encode()?;
        let hex_string: String = wasm
            .iter()
            .map(|byte| format!("0x{:02X}", byte))
            .collect::<Vec<String>>()
            .join(" ");

        assert_eq!(
            "0x00 0x61 0x73 0x6D 0x01 0x00 0x00 0x00 0x01 0x04 0x01 0x60 0x00 0x00 0x03 0x02 0x01 0x00 0x0A 0x0B 0x01 0x09 0x01 0x01 0x7F 0xFC 0x31 0x0B 0x21 0x00 0x0B 0x00 0x0F 0x04 0x6E 0x61 0x6D 0x65 0x02 0x08 0x01 0x00 0x01 0x00 0x03 0x76 0x61 0x72",
            hex_string,
        );
        Ok(())
    }

    #[test]
    fn parse_uzumaki_i64_set_module() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func
                (local $var i64)
                (local.set $var (i64.uzumaki))
            ))
        "#,
        )?;
        let mut wat = parser::parse::<Wat>(&input)?;
        let wasm = wat.encode()?;
        let hex_string: String = wasm
            .iter()
            .map(|byte| format!("0x{:02X}", byte))
            .collect::<Vec<String>>()
            .join(" ");

        assert_eq!(
            "0x00 0x61 0x73 0x6D 0x01 0x00 0x00 0x00 0x01 0x04 0x01 0x60 0x00 0x00 0x03 0x02 0x01 0x00 0x0A 0x0B 0x01 0x09 0x01 0x01 0x7E 0xFC 0x32 0x0B 0x21 0x00 0x0B 0x00 0x0F 0x04 0x6E 0x61 0x6D 0x65 0x02 0x08 0x01 0x00 0x01 0x00 0x03 0x76 0x61 0x72",
            hex_string,
        );
        Ok(())
    }
}
