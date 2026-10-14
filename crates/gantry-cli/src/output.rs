use colored::Colorize;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct Output {
    pub quiet: bool,
    pub color: bool,
    pub json: bool,
}

impl Output {
    pub fn new(quiet: bool, color: bool, json: bool) -> Self {
        Self { quiet, color, json }
    }

    pub fn print_stdout(&self, msg: &str) {
        if !self.quiet && !self.json {
            println!("{}", msg);
        }
    }

    pub fn print_stderr(&self, msg: &str) {
        eprintln!("{}", msg);
    }

    pub fn print_json<T: Serialize>(&self, val: &T) {
        if self.json {
            if let Ok(serialized) = serde_json::to_string_pretty(val) {
                println!("{}", serialized);
            }
        }
    }

    pub fn print_json_raw(&self, raw: &str) {
        if self.json {
            println!("{}", raw);
        }
    }

    pub fn heading(&self, text: &str) {
        self.print_stdout(&self.style(text, Some("cyan"), true));
    }

    pub fn field(&self, label: &str, value: impl std::fmt::Display) {
        let padded = format!("{label:<12}");
        self.print_stdout(&format!("{}{}", self.style(&padded, None, true), value));
    }

    pub fn style(&self, text: &str, color: Option<&str>, bold: bool) -> String {
        if !self.color {
            return text.to_string();
        }

        let mut styled = text.normal();
        if bold {
            styled = styled.bold();
        }

        if let Some(col) = color {
            styled = match col {
                "red" => styled.red(),
                "green" => styled.green(),
                "yellow" => styled.yellow(),
                "blue" => styled.blue(),
                "magenta" => styled.magenta(),
                "cyan" => styled.cyan(),
                "white" => styled.white(),
                _ => styled,
            };
        }

        styled.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_style() {
        colored::control::set_override(true);
        let out_color = Output::new(false, true, false);
        let styled = out_color.style("test", Some("green"), true);
        assert!(styled.contains("\u{1b}")); // contains ANSI escape codes

        colored::control::set_override(false);
        let out_no_color = Output::new(false, false, false);
        let plain = out_no_color.style("test", Some("green"), true);
        assert_eq!(plain, "test");
    }
}
