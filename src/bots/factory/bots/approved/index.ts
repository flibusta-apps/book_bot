import * as Sentry from '@sentry/node';

import { Context, Telegraf, Markup, TelegramError } from 'telegraf';

import { BotState, Cache } from '@/bots/manager';

import env from '@/config';

import * as Messages from "./messages";

import * as CallbackData from "./callback_data";

import * as BookLibrary from "./services/book_library";
import { CachedMessage, getBookCache } from './services/book_cache';
import { getBookCacheBuffer } from './services/book_cache_buffer';
import { download } from './services/downloader';
import { createOrUpdateUserSettings, getUserSettings } from './services/user_settings';
import { formatBook, formatAuthor, formatSequence, formatTranslator } from './format';
import { getPaginatedMessage, registerLanguageSettingsCallback, registerPaginationCommand, registerRandomItemCallback } from './utils';
import { getRandomKeyboard, getUpdateLogKeyboard, getUserAllowedLangsKeyboard } from './keyboard';


Sentry.init({
    dsn: env.SENTRY_DSN,
});


export async function createApprovedBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token, {
        telegram: {
            apiRoot: env.TELEGRAM_BOT_API_ROOT,
        }
    });

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

    bot.use(async (ctx: Context, next) => {
        if (ctx.from) {
            const user = ctx.from;
            createOrUpdateUserSettings({
                user_id: user.id,
                last_name: user.last_name || '',
                first_name: user.first_name,
                username: user.username || '',
                source: ctx.botInfo.username,
            });
        }
        await next();
    });

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
    registerPaginationCommand(bot, CallbackData.SEARCH_TRANSLATORS_PREFIX, BookLibrary.searchTranslators, formatTranslator);
    registerPaginationCommand(bot, CallbackData.SEARCH_AUTHORS_PREFIX, BookLibrary.searchAuthors, formatAuthor);
    registerPaginationCommand(bot, CallbackData.SEARCH_SERIES_PREFIX, BookLibrary.searchSequences, formatSequence);

    registerPaginationCommand(bot, CallbackData.AUTHOR_BOOKS_PREFIX, BookLibrary.getAuthorBooks, formatBook);
    registerPaginationCommand(bot, CallbackData.TRANSLATOR_BOOKS_PREFIX, BookLibrary.getTranslatorBooks, formatBook);
    registerPaginationCommand(bot, CallbackData.SEQUENCE_BOOKS_PREFIX, BookLibrary.getSequenceBooks, formatBook);

    bot.command("random", async (ctx: Context) => {
        ctx.reply("Что хотим получить?", {
            reply_markup: getRandomKeyboard().reply_markup,
        })
    });

    registerRandomItemCallback(bot, CallbackData.RANDOM_BOOK, BookLibrary.getRandomBook, formatBook);
    registerRandomItemCallback(bot, CallbackData.RANDOM_AUTHOR, BookLibrary.getRandomAuthor, formatAuthor);
    registerRandomItemCallback(bot, CallbackData.RANDOM_SEQUENCE, BookLibrary.getRandomSequence, formatSequence);

    bot.command("update_log", async (ctx: Context) => {
        ctx.reply("Обновление каталога: ", {
            reply_markup: getUpdateLogKeyboard().reply_markup,
        });
    });

    bot.action(new RegExp(CallbackData.UPDATE_LOG_PREFIX), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const userSettings = await getUserSettings(ctx.callbackQuery.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const data = ctx.callbackQuery.data.split("_");
        const page = parseInt(data[4]);

        const arg = `${data[2]}_${data[3]}`;

        const pMessage = await getPaginatedMessage(CallbackData.UPDATE_LOG_PREFIX, arg, page, allowedLangs, BookLibrary.getBooks, formatBook);

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard.reply_markup
        });
    });

    bot.command("settings", async (ctx: Context) => {
        const keyboard = Markup.inlineKeyboard([
            [Markup.button.callback("Языки", CallbackData.LANG_SETTINGS)]
        ]);

        ctx.reply("Настройки:", {
            reply_markup: keyboard.reply_markup
        });
    });

    bot.action(CallbackData.LANG_SETTINGS, async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const keyboard = await getUserAllowedLangsKeyboard(ctx.callbackQuery.from.id);

        ctx.editMessageText("Настройки языков:", {
            reply_markup: keyboard.reply_markup,
        });
    });

    registerLanguageSettingsCallback(bot, 'on', CallbackData.ENABLE_LANG_PREFIX);
    registerLanguageSettingsCallback(bot, 'off', CallbackData.DISABLE_LANG_PREFIX);

    bot.hears(/^\/d_[a-zA-Z0-9]+_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const [_, format, id] = ctx.message.text.split('_');

        let cache: CachedMessage;

        if (state.cache === Cache.ORIGINAL) {
            cache = await getBookCache(parseInt(id), format);
        } else if (state.cache === Cache.BUFFER) {
            cache = await getBookCacheBuffer(parseInt(id), format);
        } else {
            const book = await BookLibrary.getBookById(parseInt(id));
            const data = await download(book.source.id, book.remote_id, format);
            ctx.telegram.sendDocument(ctx.message.chat.id, data, {
                reply_to_message_id: ctx.message.message_id
            })
            return;
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

        ctx.reply(annotation.text, {
            parse_mode: "HTML",
        });
    });

    bot.hears(/^\/a_info_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const authorId = ctx.message.text.split('_')[2];

        const annotation = await BookLibrary.getAuthorAnnotation(parseInt(authorId));

        ctx.reply(annotation.text, {
            parse_mode: "HTML",
        });
    });

    bot.hears(/^\/a_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const authorId = ctx.message.text.split('_')[1];

        const userSettings = await getUserSettings(ctx.message.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const pMessage = await getPaginatedMessage(CallbackData.AUTHOR_BOOKS_PREFIX, parseInt(authorId), 1, allowedLangs, BookLibrary.getAuthorBooks, formatBook);

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard.reply_markup
        });
    });

    bot.hears(/^\/t_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const translatorId = ctx.message.text.split('_')[1];

        const userSettings = await getUserSettings(ctx.message.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const pMessage = await getPaginatedMessage(CallbackData.TRANSLATOR_BOOKS_PREFIX, parseInt(translatorId), 1, allowedLangs, BookLibrary.getTranslatorBooks, formatBook);

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard.reply_markup
        });
    });

    bot.hears(/^\/s_[\d]+$/gm, async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const sequenceId = ctx.message.text.split('_')[1];

        const userSettings = await getUserSettings(ctx.message.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const pMessage = await getPaginatedMessage(CallbackData.SEQUENCE_BOOKS_PREFIX, parseInt(sequenceId), 1, allowedLangs, BookLibrary.getSequenceBooks, formatBook);

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
                Markup.button.callback('Серию', `${CallbackData.SEARCH_SERIES_PREFIX}${query}_1`),
            ],
            [
                Markup.button.callback('Переводчика', `${CallbackData.SEARCH_TRANSLATORS_PREFIX}${query}_1`),
            ]
        ]);

        await ctx.telegram.sendMessage(ctx.message.chat.id, Messages.SEARCH_MESSAGE, {
            reply_to_message_id: ctx.message.message_id,
            reply_markup: keyboard.reply_markup,
        });
    });

    bot.catch((err) => {
        Sentry.captureException(err);
    });

    return bot;
}
