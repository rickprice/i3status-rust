use std::env;
use std::process::Command;
use std::time::Duration;

use crossbeam_channel::Sender;
use serde::Deserialize;
// use serde_derive::{Serialize, Deserialize};
use regex::Captures;
use regex::Regex;

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::de::deserialize_opt_duration;
use crate::errors::*;
use crate::protocol::i3bar_event::I3BarEvent;
use crate::scheduler::Task;
use crate::widgets::text::TextWidget;
use crate::widgets::{I3BarWidget, State};

pub struct TimeWarrior {
    id: usize,
    text: TextWidget,
    command_on: String,
    command_off: String,
    command_state: String,
    command_status_display: String,
    command_status_display_regex: Regex,
    command_status_tags_display_regex: Regex,
    icon_on: String,
    icon_off: String,
    update_interval: Option<Duration>,
    toggled: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TimeWarriorConfig {
    /// Update interval in seconds
    #[serde(default, deserialize_with = "deserialize_opt_duration")]
    pub interval: Option<Duration>,

    /// Shell Command to enable TimeWarrior time tracking
    #[serde(default = "TimeWarriorConfig::default_command_on")]
    pub command_on: String,

    /// Shell Command to disable TimeWarrior time tracking
    #[serde(default = "TimeWarriorConfig::default_command_off")]
    pub command_off: String,

    /// Shell Command to determine TimeWarrior state.
    #[serde(default = "TimeWarriorConfig::default_command_state")]
    pub command_state: String,

    /// Shell Command to determine TimeWarrior display info.
    #[serde(default = "TimeWarriorConfig::default_command_status_display")]
    pub command_status_display: String,

    #[serde(default = "TimeWarriorConfig::default_command_status_display_regex")]
    #[serde(with = "serde_regex")]
    pub command_status_display_regex: Regex,

    #[serde(default = "TimeWarriorConfig::default_command_status_tags_display_regex")]
    #[serde(with = "serde_regex")]
    pub command_status_tags_display_regex: Regex,

    /// Icon ID when time tracking is on (default is "toggle_on")
    #[serde(default = "TimeWarriorConfig::default_icon_on")]
    pub icon_on: String,

    /// Icon ID when time tracking is off (default is "toggle_off")
    #[serde(default = "TimeWarriorConfig::default_icon_off")]
    pub icon_off: String,

    /// Text to display in i3bar for this block
    pub text: Option<String>,
}

impl TimeWarriorConfig {
    fn default_command_on() -> String {
        "timew continue".to_owned()
    }

    fn default_command_off() -> String {
        "timew stop".to_owned()
    }

    fn default_command_state() -> String {
        "timew".to_owned()
    }

    fn default_command_status_display() -> String {
        "timew day".to_owned()
    }

    fn default_command_status_display_regex() -> Regex {
        Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap()
    }

    fn default_command_status_tags_display_regex() -> Regex {
        Regex::new(r"?P<tags>.*)").unwrap()
    }

    fn default_icon_on() -> String {
        "toggle_on".to_owned()
    }

    fn default_icon_off() -> String {
        "toggle_off".to_owned()
    }
}

impl ConfigBlock for TimeWarrior {
    type Config = TimeWarriorConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        _tx_update_request: Sender<Task>,
    ) -> Result<Self> {
        Ok(TimeWarrior {
            id,
            text: TextWidget::new(id, 0, shared_config)
                .with_text(&block_config.text.unwrap_or_default()),
            command_on: block_config.command_on,
            command_off: block_config.command_off,
            command_state: block_config.command_state,
            command_status_display: block_config.command_status_display,
            command_status_display_regex: block_config.command_status_display_regex,
            command_status_tags_display_regex: block_config.command_status_tags_display_regex,
            icon_on: block_config.icon_on,
            icon_off: block_config.icon_off,
            toggled: false,
            update_interval: block_config.interval,
        })
    }
}

impl Block for TimeWarrior {
    fn update(&mut self) -> Result<Option<Update>> {
        let output = Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
            .args(&["-c", &self.command_state])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|e| e.to_string());

        // I think only toggled should be set here, and icon_text should be set on the icon in its
        // own match
        let (toggled, icon_text, tags) =
            match self.command_status_tags_display_regex.captures(&output) {
                _ => (false, self.icon_off.as_str(), ""),
                Some(captures) => {
                    let tags = captures.name("tags").map_or("", |m| m.as_str());
                    (true, self.icon_on.as_str(), tags)
                }
            };

        self.toggled = toggled;
        self.text.set_icon(icon_text);

        let (icon_text, tag_data) = match output.trim_start() {
            "There is no active time tracking." => {
                self.toggled = false;
                (self.icon_off.as_str(), "")
            }
            _ => {
                self.toggled = true;
                // self.text.set_text("Is active".as_ref());
                (self.icon_on.as_str(), "Tags")
            }
        };

        self.text.set_icon(icon_text);

        // Here we need to add the Tags data and the hours data to create the output text
        self.text.set_text(match self.toggled {
            true => {
                let output = Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
                    .args(&["-c", &self.command_status_display])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                    .unwrap_or_else(|e| e.to_string());

                // So now, here we just need to crack the returned data and do something useful
                output.to_owned()
            }
            _ => "Not toggled".to_owned(),
        });

        self.text.set_state(State::Idle);

        Ok(self.update_interval.map(|d| d.into()))
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        vec![&self.text]
    }

    fn click(&mut self, _e: &I3BarEvent) -> Result<()> {
        let cmd = if self.toggled {
            &self.command_off
        } else {
            &self.command_on
        };

        let output = Command::new(env::var("SHELL").unwrap_or_else(|_| "sh".to_owned()))
            .args(&["-c", cmd])
            .output()
            .block_error("toggle", "failed to run toggle command")?;

        if output.status.success() {
            self.text.set_state(State::Idle);
            self.toggled = !self.toggled;
            self.text.set_icon(if self.toggled {
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
