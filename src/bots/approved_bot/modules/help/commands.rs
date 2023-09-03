use teloxide::utils::command::BotCommands;


#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum HelpCommand {
    Start,
    Help,
}
