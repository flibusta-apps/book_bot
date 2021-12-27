import { Context, Markup, Telegraf, TelegramError } from  'telegraf';
import { InlineKeyboardMarkup } from 'typegram';

import { getPaginationKeyboard } from './keyboard';
import * as BookLibrary from "./services/book_library";


interface PreparedMessage {
    message: string;
    keyboard: Markup.Markup<InlineKeyboardMarkup>;
}


export async function getPaginatedMessage<T>(
    prefix: string,
    data: any,
    page: number,
    itemsGetter: (data: any, page: number) => Promise<BookLibrary.Page<T>>,
    itemFormater: (item: T) => string,
): Promise<PreparedMessage> {
    const itemsPage = await itemsGetter(data, page);

    const formatedItems = itemsPage.items.map(itemFormater).join('\n\n\n');
    const message = formatedItems + `\n\nСтраница ${page}/${itemsPage.total_pages}`;

    const keyboard = getPaginationKeyboard(prefix, data, page, itemsPage.total_pages);

    return {
        message,
        keyboard
    };
} 

export function registerPaginationCommand<T>(
    bot: Telegraf,
    prefix: string,
    itemsGetter: (data: any, page: number) => Promise<BookLibrary.Page<T>>,
    itemFormater: (item: T) => string,
) {
    bot.action(new RegExp(prefix), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const [_, query, sPage] = ctx.callbackQuery.data.split('_');

        const pMessage = await getPaginatedMessage(prefix, query, parseInt(sPage), itemsGetter, itemFormater);

        try {
            await ctx.editMessageText(pMessage.message, {
                reply_markup: pMessage.keyboard.reply_markup
            });
        } catch (err) {
            console.log(err);
        }
    })
}
