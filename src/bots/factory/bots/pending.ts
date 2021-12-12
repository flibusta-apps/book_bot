import { Telegraf, Context } from 'telegraf';

import { BotState } from '@/bots/manager';


export async function createPendingBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token);

    await bot.telegram.deleteMyCommands();

    bot.on("message", async (ctx: Context) => {
        await ctx.reply('Бот зарегистрирован, но не подтвержден администратором!');
    });

    return bot;
}
