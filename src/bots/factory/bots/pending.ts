import { Telegraf, Context } from 'telegraf';

import { BotState } from '@/bots/manager';

import env from '@/config';


export async function createPendingBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token, {
        telegram: {
            apiRoot: env.TELEGRAM_BOT_API_ROOT,
        }
    });

    await bot.telegram.deleteMyCommands();

    bot.on("message", async (ctx: Context) => {
        await ctx.reply(
            'Бот зарегистрирован, но не подтвержден администратором! \n' +
            'Подтверждение занимает примерно 12 часов.'
        );
    });

    return bot;
}
