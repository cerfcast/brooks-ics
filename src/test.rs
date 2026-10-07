// brooks-ics, Copyright 2026, Will Hawkins
//
// This file is part of brooks-ics.

// This file is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

macro_rules! apply_binary_name_filters {
    {} => {
        let mut settings = insta::Settings::clone_current();
        // CLI Binary Name
        settings.add_filter(r"brooks-ics(\.exe)?", "[BROOKS_ICS]");
        let _bound = settings.bind_to_scope();
    }
}

#[cfg(test)]
mod cli_tests {
    use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
    use std::process::Command;

    #[test]
    fn help_test() {
        apply_binary_name_filters!();
        assert_cmd_snapshot!(Command::new(get_cargo_bin("brooks-ics")).arg("--help"));
    }

    #[cfg(not(feature = "domain"))]
    #[test]
    fn no_domain_feature_test() {
        apply_binary_name_filters!();
        assert_cmd_snapshot!(Command::new(get_cargo_bin("brooks-ics")).args([
            "hmds",
            "--port",
            "8081",
            "--timeout",
            "25s",
            "--user",
            "testing_user",
        ]));
    }

    #[cfg(feature = "domain")]
    #[test]
    fn domain_feature_test() {
        apply_binary_name_filters!();
        assert_cmd_snapshot!(Command::new(get_cargo_bin("brooks-ics")).args([
            "hmds",
            "--port",
            "8081",
            "--timeout",
            "25s",
            "--path",
            "/tmp/todo-use-actual-unique-value",
            "--user",
            "testing_user",
        ]));
    }
}
