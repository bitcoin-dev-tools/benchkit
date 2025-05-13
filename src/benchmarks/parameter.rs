use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parameter list from the benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterList {
    /// The variable name to use in command templates
    pub var: String,
    /// The values to substitute for the variable
    pub values: Vec<String>,
}

/// A parameter matrix that contains all combinations of parameters
#[derive(Debug, Clone)]
pub struct ParameterMatrix {
    /// The list of parameter combinations
    pub combinations: Vec<HashMap<String, String>>,
}

impl ParameterMatrix {
    /// Create a new parameter matrix from a list of parameter lists
    pub fn new(parameter_lists: &[ParameterList]) -> Self {
        if parameter_lists.is_empty() {
            // If there are no parameter lists, create a single empty combination
            return Self {
                combinations: vec![HashMap::new()],
            };
        }

        // Start with a single empty combination
        let mut combinations = vec![HashMap::new()];

        // For each parameter list, create new combinations
        for param_list in parameter_lists {
            let mut new_combinations = Vec::new();

            // For each existing combination and each value in the parameter list,
            // create a new combination
            for combination in combinations {
                for value in &param_list.values {
                    let mut new_combination = combination.clone();
                    new_combination.insert(param_list.var.clone(), value.clone());
                    new_combinations.push(new_combination);
                }
            }

            combinations = new_combinations;
        }

        Self { combinations }
    }

    /// Apply a parameter combination to a command template
    pub fn apply_parameters(
        &self,
        command_template: &str,
        params: &HashMap<String, String>,
    ) -> String {
        let mut command = command_template.to_string();

        for (var, value) in params {
            let placeholder = format!("{{{}}}", var);
            command = command.replace(&placeholder, value);
        }

        command
    }

    /// Generate all commands from a template and parameter matrix
    pub fn generate_commands(
        &self,
        command_template: &str,
    ) -> Vec<(String, HashMap<String, String>)> {
        let mut commands = Vec::new();

        for params in &self.combinations {
            let command = self.apply_parameters(command_template, params);
            commands.push((command, params.clone()));
        }

        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_matrix_empty() {
        let matrix = ParameterMatrix::new(&[]);
        assert_eq!(matrix.combinations.len(), 1);
        assert_eq!(matrix.combinations[0].len(), 0);
    }

    #[test]
    fn test_parameter_matrix_single() {
        let param_list = ParameterList {
            var: "foo".to_string(),
            values: vec!["a".to_string(), "b".to_string()],
        };
        let matrix = ParameterMatrix::new(&[param_list]);
        assert_eq!(matrix.combinations.len(), 2);
        assert_eq!(matrix.combinations[0].get("foo"), Some(&"a".to_string()));
        assert_eq!(matrix.combinations[1].get("foo"), Some(&"b".to_string()));
    }

    #[test]
    fn test_parameter_matrix_multiple() {
        let param_list1 = ParameterList {
            var: "foo".to_string(),
            values: vec!["a".to_string(), "b".to_string()],
        };
        let param_list2 = ParameterList {
            var: "bar".to_string(),
            values: vec!["1".to_string(), "2".to_string()],
        };
        let matrix = ParameterMatrix::new(&[param_list1, param_list2]);

        assert_eq!(matrix.combinations.len(), 4);

        // Check all combinations
        let combinations = [("a", "1"), ("a", "2"), ("b", "1"), ("b", "2")];

        for (i, (foo, bar)) in combinations.iter().enumerate() {
            assert_eq!(matrix.combinations[i].get("foo"), Some(&foo.to_string()));
            assert_eq!(matrix.combinations[i].get("bar"), Some(&bar.to_string()));
        }
    }

    #[test]
    fn test_apply_parameters() {
        let matrix = ParameterMatrix::new(&[]);
        let mut params = HashMap::new();
        params.insert("foo".to_string(), "bar".to_string());
        params.insert("baz".to_string(), "qux".to_string());

        let command = matrix.apply_parameters("test {foo} command {baz}", &params);
        assert_eq!(command, "test bar command qux");
    }

    #[test]
    fn test_generate_commands() {
        let param_list = ParameterList {
            var: "foo".to_string(),
            values: vec!["a".to_string(), "b".to_string()],
        };
        let matrix = ParameterMatrix::new(&[param_list]);

        let commands = matrix.generate_commands("test {foo} command");
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].0, "test a command");
        assert_eq!(commands[1].0, "test b command");
    }
}
