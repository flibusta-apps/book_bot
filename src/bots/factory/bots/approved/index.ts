import { Context, Telegraf, Markup } from 'telegraf';

import { BotState } from '@/bots/manager';

import env from '@/config';

import * as Messages from "./messages";

import * as CallbackData from "./callback_data";

import * as BookLibrary from "./services/book_library";
import { CachedMessage, getBookCache } from './services/book_cache';
import { getBookCacheBuffer } from './services/book_cache_buffer';
import { formatBook, formatAuthor, formatSequence } from './format';
import { getPaginatedMessage, registerPaginationCommand } from './utils';


export async function createApprovedBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token, {
        telegram: {
            apiRoot: env.TELEGRAM_BOT_API_ROOT,
        }
    });

    await bot.telegram.setMyCommands([
        {command: "random_book", description: "Случайная книга"},
        {command: "update_log", description: "Информация об обновлении каталога"},
        {command: "settings", description: "Настройки"},
        {command: "help", description: "Помощь"},
    ]);

    bot.help((ctx: Context) => ctx.reply(Messages.HELP_MESSAGE));

    bot.start((ctx: Context) => {
        if (!ctx.message) {
            return;
        }

        const name = ctx.message.from.first_name || ctx.message.from.username || 'пользователь';
        ctx.telegram.sendMessage(ctx.message.chat.id,
            Messages.START_MESSAGE.replace('{name}', name), {
                reply_to_message_id: ctx.message.message_id,
            }
        );
    });

    registerPaginationCommand(bot, CallbackData.SEARCH_BOOK_PREFIX, BookLibrary.searchByBookName, formatBook);
    registerPaginationCommand(bot, CallbackData.SEARCH_AUTHORS_PREFIX, BookLibrary.searchAuthors, formatAuthor);
    registerPaginationCommand(bot, CallbackData.SEARCH_SERIES_PREFIX, BookLibrary.searchSequences, formatSequence);
    registerPaginationCommand(bot, CallbackData.AUTHOR_BOOKS_PREFIX, BookLibrary.getAuthorBooks, formatBook);
    registerPaginationCommand(bot, CallbackData.SEQUENCE_BOOKS_PREFIX, BookLibrary.getSequenceBooks, formatBook);

    bot.hears(/^\/d_[a-zA-Z0-9]+_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const [_, format, id] = ctx.message.text.split('_');

        let cache: CachedMessage;

        if (state.privileged) {
            cache = await getBookCache(parseInt(id), format);
        } else {
            cache = await getBookCacheBuffer(parseInt(id), format);
        }

        ctx.telegram.copyMessage(ctx.message.chat.id, cache.chat_id, cache.message_id, {
            allow_sending_without_reply: true,
        })
    });

    bot.hears(/^\/b_info_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const bookId = ctx.message.text.split('_')[2];

        const annotation = await BookLibrary.getBookAnnotation(parseInt(bookId));

        ctx.reply(annotation.text);
    });

    bot.hears(/^\/a_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const authorId = ctx.message.text.split('_')[1];

        const pMessage = await getPaginatedMessage(CallbackData.AUTHOR_BOOKS_PREFIX, authorId, 1, BookLibrary.getAuthorBooks, formatBook);

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard.reply_markup
        });
    });

    bot.hears(/^\/s_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const sequenceId = ctx.message.text.split('_')[1];

        const pMessage = await getPaginatedMessage(CallbackData.SEQUENCE_BOOKS_PREFIX, sequenceId, 1, BookLibrary.getSequenceBooks, formatBook);

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard.reply_markup
        });
    });

    bot.on("message", async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const query = ctx.message.text.substring(0, 64 - 7);

        let keyboard = Markup.inlineKeyboard([
            [
                Markup.button.callback('Книгу', `${CallbackData.SEARCH_BOOK_PREFIX}${query}_1`)
            ],
            [
                Markup.button.callback('Автора',  `${CallbackData.SEARCH_AUTHORS_PREFIX}${query}_1`),
            ],
            [
                Markup.button.callback('Серию', `${CallbackData.SEARCH_SERIES_PREFIX}${query}_1`)
            ],
            [
                Markup.button.callback('Переводчика', '# ToDO'),
            ]
        ]);

        await ctx.telegram.sendMessage(ctx.message.chat.id, Messages.SEARCH_MESSAGE, {
            reply_to_message_id: ctx.message.message_id,
            reply_markup: keyboard.reply_markup,
        });
    }); 

    return bot;
}
