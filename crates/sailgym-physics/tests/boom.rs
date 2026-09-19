mod rigging {
    mod boom {
        #[test]
        fn no_tack_state() {
            fn check(dir: &std::path::Path) {
                for entry in std::fs::read_dir(dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        check(&path);
                    } else if path.extension().is_some_and(|x| x == "rs") {
                        let source = std::fs::read_to_string(&path).unwrap().to_lowercase();
                        for forbidden in ["porttack", "starboardtack", "tack_state", "on_port"] {
                            assert!(
                                !source.contains(forbidden),
                                "{}: {forbidden}",
                                path.display()
                            );
                        }
                    }
                }
            }
            check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        }
    }
}
