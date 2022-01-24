import { Telegraf, Context } from 'telegraf';

import { BotState } from '@/bots/manager';

import env from '@/config';


export async function createBlockedBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token, {
        telegram: {
            apiRoot: env.TELEGRAM_BOT_API_ROOT,
        }
    });

    await bot.telegram.deleteMyCommands();

    bot.on("message", async (ctx: Context) => {
        await ctx.reply('Бот заблокирован!');
    });

    return bot;
}
