use teloxide::macros::BotCommands;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum SettingsCommand {
    Settings,
}
