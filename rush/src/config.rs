// CONFIGURATION MANAGEMENT
//
// This module defines a struct to represent engine configurations and
// functions to add apps and links to a configuration.
//
//   Config - inspectable, cloneable configuration structure
//   new() -> Config - Create a new empty configuration
//   app(&mut Config, name:&str, &AppConfig) - Add an app to a configuration
//   link(&mut Config, linkspec:&str) - Add a link to a configuration

use super::engine;

use std::collections::HashMap;
use std::collections::HashSet;
use regex::Regex;
use once_cell::sync::Lazy;

// Config can be applied by engine.
#[derive(Clone)]
pub struct Config {
    pub apps: HashMap<String, Box<dyn engine::AppArg>>,
    pub links: HashSet<String>
}

// API: Create a new configuration.
// Initially there are no apps or links.
pub fn new() -> Config {
    Config { apps: HashMap::new(), links: HashSet::new() }
}

// API: Add an app to the configuration.
//
// config::app(c, name, app):
//   c is a Config object.
//   name is the name of this app in the network.
//   app is the engine::AppConfig object used to create the app instance.
//
// Example: config::app(&mut c, "source", &basic_apps::Source {size: 60})
pub fn app(config: &mut Config, name: &str, app: &dyn engine::AppArg) {
    config.apps.insert(name.to_string(), app.box_clone());
}

// API: Add a link to the configuration.
//
// Example: config::link(&mut c, "nic.tx -> vm.rx")
pub fn link(config: &mut Config, spec: &str) {
    config.links.insert(canonical_link(spec));
}

// Given "a.out -> b.in" return
//   LinkSpec { from: "a", output:"out", to: "b", input: "in" }.
pub fn parse_link(spec: &str) -> LinkSpec {
    if let Some(cap) = LINK_SYNTAX.captures(spec) {
        LinkSpec {
            from: (&cap[1]).to_string(), output: (&cap[2]).to_string(),
            to: (&cap[3]).to_string(), input: (&cap[4]).to_string(),
        }
    } else {
        panic!("link parse error: {}", spec)
    }
}

pub struct LinkSpec {
    pub from: String, pub output: String,
    pub to: String, pub input: String
}

static LINK_SYNTAX: Lazy<Regex> = Lazy::new
    (|| Regex::new(r" *([\w_]+)\.([\w_]+) *-> *([\w_]+)\.([\w_]+) *").unwrap());

fn format_link(spec: &LinkSpec) -> String {
    format!("{}.{} -> {}.{}", spec.from, spec.output, spec.to, spec.input)
}

fn canonical_link(spec: &str) -> String {
    format_link(&parse_link(spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic_apps;

    #[test]
    fn config () {
        let mut c = new();
        println!("Created an empty configuration");
        app(&mut c, "source", &basic_apps::Source {size: 60});
        println!("Added an app");
        link(&mut c, "source.output -> sink.input");
        println!("Added an link");
    }

}
