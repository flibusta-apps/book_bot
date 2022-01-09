import { Telegraf, TelegramError } from "telegraf";


export async function setCommands(bot: Telegraf) {
    async function setMyCommands() {
        await bot.telegram.setMyCommands([
            {command: "random", description: "Попытать удачу"},
            {command: "update_log", description: "Обновления каталога"},
            {command: "settings", description: "Настройки"},
            {command: "help", description: "Помощь"},
        ]);
    }

    try {
        await setMyCommands();
    } catch (e: unknown) {
        if (e instanceof TelegramError && e.response.error_code === 429) {
            setTimeout(() => setMyCommands(), 1000 * (e.response.parameters?.retry_after || 5));
        }
    }
}