//! Headless entry points for the system API, independent of GUI/Settings IPC.
use std::io::Read;
pub fn run(args: &[String]) -> anyhow::Result<bool> {
    match args {
        [flag] if flag == "--system-info" => {
            let api = macstill_system::SystemApi::new();
            println!("{}", serde_json::to_string_pretty(&api.snapshot())?);
            Ok(true)
        }
        [flag, file] if flag == "--system-query" => {
            let reader = input(file)?;
            let query = parse_request::<macstill_system::SystemQuery>(reader)?;
            let api = macstill_system::SystemApi::new();
            println!("{}", serde_json::to_string_pretty(&api.query(query)?)?);
            Ok(true)
        }
        [flag, file] if flag == "--system-apply" => {
            let command = parse_command(input(file)?)?;
            let api = macstill_system::SystemApi::new();
            api.apply(command)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"accepted":true,"state":api.snapshot()})
                )?
            );
            Ok(true)
        }
        _ => Ok(false),
    }
}
fn input(file: &str) -> anyhow::Result<Box<dyn Read>> {
    Ok(if file == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(file)?)
    })
}
fn parse_command(reader: impl Read) -> anyhow::Result<macstill_system::SystemCommand> {
    parse_request(reader)
}
fn parse_request<T: serde::de::DeserializeOwned>(reader: impl Read) -> anyhow::Result<T> {
    const MAX: u64 = 16 * 1024;
    let mut bytes = vec![];
    reader.take(MAX + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX,
        "System request must be at most 16 KiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_input_rejects_oversized_or_unknown_operations() {
        assert!(parse_command(&vec![b' '; 16 * 1024 + 1][..]).is_err());
        assert!(
            parse_command(br#"{"operation":"run_shell","value":"echo bad"}"#.as_slice()).is_err()
        );
        assert!(
            parse_command(br#"{"operation":"set_wifi","value":true,"typo":1}"#.as_slice()).is_err()
        );
        assert!(matches!(
            parse_command(br#"{"operation":"set_wifi","value":true}"#.as_slice()).unwrap(),
            macstill_system::SystemCommand::SetWifi(true)
        ));
    }
    #[test]
    fn queries_are_read_only_and_use_the_same_input_limit() {
        assert!(
            parse_request::<macstill_system::SystemQuery>(&vec![b' '; 16 * 1024 + 1][..]).is_err()
        );
        assert!(
            parse_request::<macstill_system::SystemQuery>(
                br#"{"operation":"set_wifi","value":false}"#.as_slice()
            )
            .is_err()
        );
        assert!(matches!(
            parse_request::<macstill_system::SystemQuery>(
                br#"{"operation":"timezones"}"#.as_slice()
            )
            .unwrap(),
            macstill_system::SystemQuery::Timezones
        ));
    }
}
