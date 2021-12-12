import { Telegraf, Context } from 'telegraf';

import { BotState } from '@/bots/manager';


export async function createBlockedBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token);

    await bot.telegram.deleteMyCommands();

    bot.on("message", async (ctx: Context) => {
        await ctx.reply('Бот заблокирован!');
    });

    return bot;
}
