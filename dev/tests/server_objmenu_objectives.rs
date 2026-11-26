//! `bingo objectives on|off`: the parsing, the operator rule, the usage text, the saved form of the option, and the
//! `bingo options` line that names it when it is on. The expected texts are written out by hand.
use horizon_bingo::server::commands::{parse, requires_op, usage, Command};
use horizon_bingo::server::options::{Options, Toggle};
use horizon_bingo::server::store::{options_from_json, options_to_json};
use serde_json::Value;

#[test]
fn objectives_is_an_on_off_toggle() {
    assert_eq!(
        parse("objectives on"),
        Ok(Command::Toggle(Toggle::Objectives, true))
    );
    assert_eq!(
        parse("objectives OFF"),
        Ok(Command::Toggle(Toggle::Objectives, false))
    );
    assert_eq!(
        parse("objectives true"),
        Ok(Command::Toggle(Toggle::Objectives, true))
    );
    assert_eq!(
        parse("objectives maybe").unwrap_err(),
        "objectives needs on or off, got 'maybe'"
    );
    assert_eq!(parse("objectives").unwrap_err(), "Usage: objectives on|off");
    assert_eq!(
        parse("objectives on now").unwrap_err(),
        "Too many arguments for 'objectives'. Usage: objectives on|off"
    );
}

#[test]
fn objectives_needs_an_operator_and_is_in_the_usage_text() {
    assert!(requires_op(&parse("objectives on").unwrap()));
    assert!(requires_op(&parse("objectives off").unwrap()));
    assert!(usage().contains("objectives on|off"), "{}", usage());
}

#[test]
fn set_toggle_sets_the_objectives_flag_and_leaves_the_others() {
    let mut options = Options::default();
    options.set_toggle(Toggle::Objectives, true);
    assert!(options.objectives);
    assert!(options.pvp && !options.elytra && !options.team_auto_shuffle);
    options.set_toggle(Toggle::Objectives, false);
    assert!(!options.objectives);
}

fn keys(json: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(json).expect("options are JSON");
    value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn objectives_is_saved_only_when_on_and_read_back() {
    let off = options_to_json(&Options::default());
    assert!(
        !keys(&off).iter().any(|key| key == "objectives"),
        "absent by default: {off}"
    );

    let on = Options {
        objectives: true,
        ..Options::default()
    };
    let text = options_to_json(&on);
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["objectives"], Value::Bool(true));
    assert!(options_from_json(&text).objectives);
    assert!(!options_from_json(&off).objectives);
}

mod session_side {
    //! The round side: the flag shows in `bingo options` only when on, and only before the start.
    use std::collections::HashMap;

    use horizon_bingo::server::game::GameOptions;
    use horizon_bingo::server::host::ServerHost;
    use horizon_bingo::server::session::{Session, SessionOptions};
    use horizon_bingo::server::types::Player;

    const ALICE: u128 = 1;
    const BOB: u128 = 2;

    #[derive(Default)]
    struct FakeHost {
        online: Vec<Player>,
        files: HashMap<String, String>,
    }

    impl ServerHost for FakeHost {
        fn send(&mut self, _player: u128, _path: &str, _payload: Vec<u8>) {}

        fn tell(&mut self, _player: u128, _line: &str) {}

        fn broadcast(&mut self, _line: &str) {}

        fn players(&self) -> Vec<Player> {
            self.online.clone()
        }

        fn item_numeric_id(&self, item: &str) -> Option<i32> {
            Some(item.len() as i32)
        }

        fn command(&mut self, _line: &str) {}

        fn read_file(&mut self, name: &str) -> Option<String> {
            self.files.get(name).cloned()
        }

        fn write_file(&mut self, name: &str, content: &str) {
            self.files.insert(name.to_string(), content.to_string());
        }
    }

    fn lobby() -> (FakeHost, Session) {
        let mut host = FakeHost {
            online: vec![
                Player {
                    uuid: ALICE,
                    name: "Alice".into(),
                },
                Player {
                    uuid: BOB,
                    name: "Bob".into(),
                },
            ],
            ..FakeHost::default()
        };
        let mut session = Session::new(SessionOptions {
            seed: 7,
            game: GameOptions {
                countdown_secs: 0,
                time_limit_secs: None,
            },
            ..SessionOptions::default()
        });
        for player in host.online.clone() {
            session.on_join(&mut host, &player);
        }
        assert!(session
            .command(&mut host, Some(ALICE), "join red")
            .starts_with("Moved player"));
        assert!(session
            .command(&mut host, Some(BOB), "join blue")
            .starts_with("Moved player"));
        (host, session)
    }

    const DEFAULT_OPTIONS: &str = "Goal: 1 line(s). Mode: standard. PvP: on. Keep inventory: off. Elytra: off. Night vision: off. Spawn distance: 25 chunks.";

    #[test]
    fn the_options_line_names_objectives_only_when_it_is_on() {
        let (mut host, mut session) = lobby();
        assert_eq!(
            session.command(&mut host, Some(ALICE), "options"),
            DEFAULT_OPTIONS
        );

        let reply = session.command(&mut host, Some(ALICE), "objectives on");
        assert_eq!(reply, format!("{DEFAULT_OPTIONS} Objectives: on."));
        assert!(session.rules().objectives);
        assert!(
            host.files
                .values()
                .any(|text| text.contains("\"objectives\": true")),
            "the option is saved"
        );

        assert_eq!(
            session.command(&mut host, Some(ALICE), "objectives off"),
            DEFAULT_OPTIONS
        );
        assert!(!session.rules().objectives);
    }

    #[test]
    fn objectives_cannot_change_once_the_round_has_started() {
        let (mut host, mut session) = lobby();
        assert_eq!(session.command(&mut host, None, "start"), "Starting the game...");
        assert_eq!(
            session.command(&mut host, Some(ALICE), "objectives on"),
            "The setting can be changed only before the game starts."
        );
        assert!(!session.rules().objectives);
    }
}
