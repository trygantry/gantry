use std::io::IsTerminal;

pub trait Prompter {
    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String, String>;
    fn select(&self, prompt: &str, choices: &[&str], default: usize) -> Result<usize, String>;
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, String>;
}

pub struct TerminalPrompter;

impl Prompter for TerminalPrompter {
    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String, String> {
        if !std::io::stdin().is_terminal() {
            return Err("Input is not a TTY. Interactive prompts require a terminal.".to_string());
        }
        let mut input = dialoguer::Input::<String>::new().with_prompt(prompt);
        if let Some(def) = default {
            input = input.default(def.to_string());
        }
        input.interact_text().map_err(|e| e.to_string())
    }

    fn select(&self, prompt: &str, choices: &[&str], default: usize) -> Result<usize, String> {
        if !std::io::stdin().is_terminal() {
            return Err("Input is not a TTY. Interactive prompts require a terminal.".to_string());
        }
        dialoguer::Select::new()
            .with_prompt(prompt)
            .items(choices)
            .default(default)
            .interact()
            .map_err(|e| e.to_string())
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, String> {
        if !std::io::stdin().is_terminal() {
            return Err("Input is not a TTY. Interactive prompts require a terminal.".to_string());
        }
        dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub struct FakePrompter {
    pub inputs: std::cell::RefCell<Vec<String>>,
    pub selects: std::cell::RefCell<Vec<usize>>,
    pub confirms: std::cell::RefCell<Vec<bool>>,
}

#[cfg(test)]
impl Prompter for FakePrompter {
    fn input(&self, _prompt: &str, default: Option<&str>) -> Result<String, String> {
        let mut inputs = self.inputs.borrow_mut();
        if !inputs.is_empty() {
            Ok(inputs.remove(0))
        } else if let Some(def) = default {
            Ok(def.to_string())
        } else {
            Err("No more mock inputs available".to_string())
        }
    }

    fn select(&self, _prompt: &str, _choices: &[&str], _default: usize) -> Result<usize, String> {
        let mut selects = self.selects.borrow_mut();
        if !selects.is_empty() {
            Ok(selects.remove(0))
        } else {
            Err("No more mock select choices available".to_string())
        }
    }

    fn confirm(&self, _prompt: &str, _default: bool) -> Result<bool, String> {
        let mut confirms = self.confirms.borrow_mut();
        if !confirms.is_empty() {
            Ok(confirms.remove(0))
        } else {
            Err("No more mock confirmation inputs available".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_prompter() {
        let prompter = FakePrompter {
            inputs: std::cell::RefCell::new(vec!["input1".to_string()]),
            selects: std::cell::RefCell::new(vec![1]),
            confirms: std::cell::RefCell::new(vec![true]),
        };

        assert_eq!(prompter.input("Prompt", None), Ok("input1".to_string()));
        assert_eq!(prompter.select("Prompt", &["a", "b"], 0), Ok(1));
        assert_eq!(prompter.confirm("Prompt", false), Ok(true));
    }
}
