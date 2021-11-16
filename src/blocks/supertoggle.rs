use std::collections::HashMap;
use std::env;
use std::fmt::Debug;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde::Deserialize;
// use serde_derive::{Serialize, Deserialize};
use regex::Regex;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_opt_duration;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct SuperToggle {
    id: usize,
    text: TextWidget,
    command_on: String,
    command_off: String,
    command_current_state: String,
    format_on: FormatTemplate,
    format_off: FormatTemplate,
    command_data_on_regex: Regex,
    command_data_off_regex: Regex,
    icon_on: String,
    icon_off: String,
    update_interval: Option<Duration>,
    // toggled: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SuperToggleConfig {
    /// Update interval in seconds
    #[serde(default, deserialize_with = "deserialize_opt_duration")]
    pub interval: Option<Duration>,

    /// Shell Command to determine SuperToggle state.
    #[serde(default = "SuperToggleConfig::default_command_current_state")]
    pub command_current_state: String,

    /// Shell Command to enable SuperToggle time tracking
    #[serde(default = "SuperToggleConfig::default_command_on")]
    pub command_on: String,

    /// Shell Command to disable SuperToggle time tracking
    #[serde(default = "SuperToggleConfig::default_command_off")]
    pub command_off: String,

    /// Format override
    pub format_on: FormatTemplate,

    /// Format override
    pub format_off: FormatTemplate,

    #[serde(default = "SuperToggleConfig::default_command_data_on_regex")]
    #[serde(with = "serde_regex")]
    pub command_data_on_regex: Regex,

    #[serde(default = "SuperToggleConfig::default_command_data_off_regex")]
    #[serde(with = "serde_regex")]
    pub command_data_off_regex: Regex,

    /// Icon ID when time tracking is on (default is "toggle_on")
    #[serde(default = "SuperToggleConfig::default_icon_on")]
    pub icon_on: String,

    /// Icon ID when time tracking is off (default is "toggle_off")
    #[serde(default = "SuperToggleConfig::default_icon_off")]
    pub icon_off: String,

    /// Text to display in i3bar for this block
    pub text: Option<String>,
}

impl SuperToggleConfig {
    fn default_command_on() -> String {
        "timew continue".to_owned()
    }

    fn default_command_off() -> String {
        "timew stop".to_owned()
    }

    fn default_command_current_state() -> String {
        "timew".to_owned()
    }

    fn default_command_status_display() -> String {
        "timew day".to_owned()
    }

    fn default_command_data_on_regex() -> Regex {
        Regex::new(r"(?m)Tracked\s+(\d{1,2}:\d{1,2}:\d{1,2})").unwrap()
    }

    fn default_command_data_off_regex() -> Regex {
        Regex::new(r"(?m)Tracked\s+(\d{1,2}:\d{1,2}:\d{1,2})").unwrap()
    }

    // fn default_command_status_display_regex() -> Regex {
    //     Regex::new(r"(?m)Tracked\s+(\d{1,2}:\d{1,2}:\d{1,2})").unwrap()
    // }

    // fn default_command_status_tags_display_regex() -> Regex {
    //     Regex::new(r"Tracking (.+)\n").unwrap()
    // }

    fn default_icon_on() -> String {
        "toggle_on".to_owned()
    }

    fn default_icon_off() -> String {
        "toggle_off".to_owned()
    }
}

impl ConfigBlock for SuperToggle {
    type Config = SuperToggleConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(SuperToggle {
            id,
            text: TextWidget::new(id, 0, shared_config)
                .with_text(&block_config.text.unwrap_or_default()),
            command_on: block_config.command_on,
            command_off: block_config.command_off,
            format_on: block_config
                .format_on
                .with_default("TW [ {tags} ] {hours}:{minutes}")?,
            format_off: block_config.format_off.with_default("TW IDLE")?,
            command_current_state: block_config.command_current_state,
            command_data_on_regex: block_config.command_data_on_regex,
            command_data_off_regex: block_config.command_data_off_regex,
            icon_on: block_config.icon_on,
            icon_off: block_config.icon_off,
            update_interval: block_config.interval,
        })
    }
}

fn get_output_of_command(command: &str) -> Result<String> {
    Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
        .args(&["-c", command])
        .output()
        .map(|o| Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned()))?
}

fn get_mapped_matches_from_string(totest: &str, regex: &Regex) -> Option<HashMap<String, Value>> {
    Some(map!(
        "testing".to_owned() => Value::from_string("testvalue".to_owned()),
    ))
}

impl SuperToggle {
    fn is_on_status(&self) -> Result<(bool, HashMap<String, Value>)> {
        let output = get_output_of_command(&self.command_current_state)?;

        match get_mapped_matches_from_string(&output, &self.command_data_on_regex) {
            Some(x) => Ok((true, x)),
            None => match get_mapped_matches_from_string(&output, &self.command_data_off_regex) {
                Some(x) => Ok((false, x)),
                None => Err(BlockError(
                    "is_on_status".to_owned(),
                    "Unable to match either the command_data_on or the command_data_off regex"
                        .to_owned(),
                )),
            },
        }
    }
}

impl Block for SuperToggle {
    fn update(&mut self) -> Result<Option<Update>> {
        let (on, tags) = &self.is_on_status()?;

        self.text.set_icon(match on {
            true => self.icon_on.as_str(),
            false => self.icon_off.as_str(),
        })?;

        let output = match on {
            true => self.format_on.render(tags),
            false => self.format_off.render(tags),
        }?;

        self.text.set_texts(output);

        self.text.set_state(State::Idle);

        Ok(self.update_interval.map(|d| d.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _e: &I3BarEvent) -> Result<()> {
        let (on, _) = self.is_on_status()?;

        let cmd = if on {
            &self.command_off
        } else {
            &self.command_on
        };

        let output =
            get_output_of_command(cmd).block_error("toggle", "Failed to run toggle command");

        if output.is_ok() {
            self.text.set_state(State::Idle);
            // self.text.set_text("Updating...".to_owned());

            self.update()?;

            // Whatever we were, we are now the opposite, so set the icon appropriately
            self.text.set_icon(if !on {
                self.icon_on.as_str()
            } else {
                self.icon_off.as_str()
            })?
        } else {
            self.text.set_state(State::Critical);
        };

        Ok(())
    }

    fn id(&self) -> usize {
        self.id
    }
}
