import * as Sentry from '@sentry/node';

import { Context, Telegraf, Markup } from 'telegraf';
import moment from 'moment';

import { BotState } from '@/bots/manager/types';

import env from '@/config';

import * as Messages from "./messages";

import * as CallbackData from "./callback_data";

import * as BookLibrary from "./services/book_library";
import UsersCounter from '@/analytics/users_counter';
import { createOrUpdateUserSettings, getUserSettings } from './services/user_settings';
import { formatBook, formatBookShort, formatAuthor, formatSequence, formatTranslator, formatDetailBook } from './format';
import { getCallbackArgs, getPaginatedMessage, getPrefixWithQueryCreator, getSearchArgs, registerLanguageSettingsCallback, registerPaginationCommand, registerRandomItemCallback } from './utils';
import { getRandomKeyboard, getTextPaginationData, getUpdateLogKeyboard, getUserAllowedLangsKeyboard } from './keyboard';
import { sendFile } from './hooks/downloading';
import { setCommands } from './hooks/setCommands';
import { downloadImage } from './services/downloader';
import { isNotModifiedMessage, isReplyMessageNotFound } from './errors_utils';


Sentry.init({
    dsn: env.SENTRY_DSN,
});


export async function createApprovedBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token, {
        telegram: {
            apiRoot: env.TELEGRAM_BOT_API_ROOT,
        }
    });

    const me = await bot.telegram.getMe();

    setCommands(bot);

    bot.use(async (ctx: Context, next) => {
        if (ctx.from) {
            const user = ctx.from;
            createOrUpdateUserSettings({
                user_id: user.id,
                last_name: user.last_name || '',
                first_name: user.first_name,
                username: user.username || '',
                source: me.username,
            });
            UsersCounter.take(user.id, me.username);
        }
        await next();
    });

    bot.command(["start", `start@${me.username}`], async (ctx: Context) => {
        if (!ctx.message) {
            return;
        }

        const name = ctx.message.from.first_name || ctx.message.from.username || 'пользователь';
        await ctx.telegram.sendMessage(ctx.message.chat.id,
            Messages.START_MESSAGE.replace('{name}', name), {
                reply_to_message_id: ctx.message.message_id,
            }
        );
    });

    bot.command(["help", `help@${me.username}`], async (ctx: Context) => ctx.reply(Messages.HELP_MESSAGE));

    registerPaginationCommand(
        bot, CallbackData.SEARCH_BOOK_PREFIX, getSearchArgs, null, BookLibrary.searchByBookName, formatBookShort, undefined, Messages.BOOKS_NOT_FOUND
    );
    registerPaginationCommand(
        bot, CallbackData.SEARCH_TRANSLATORS_PREFIX, getSearchArgs, null, BookLibrary.searchTranslators, formatTranslator,
        undefined, Messages.TRANSLATORS_NOT_FOUND
    );
    registerPaginationCommand(
        bot, CallbackData.SEARCH_AUTHORS_PREFIX, getSearchArgs, null, BookLibrary.searchAuthors, formatAuthor, undefined, Messages.AUTHORS_NOT_FOUND
    );
    registerPaginationCommand(
        bot, CallbackData.SEARCH_SERIES_PREFIX, getSearchArgs, null, BookLibrary.searchSequences, formatSequence, undefined, Messages.SEQUENCES_NOT_FOUND
    );

    registerPaginationCommand(
        bot, CallbackData.AUTHOR_BOOKS_PREFIX, getCallbackArgs, getPrefixWithQueryCreator(CallbackData.AUTHOR_BOOKS_PREFIX),
        BookLibrary.getAuthorBooks, formatBookShort, undefined, Messages.BOOKS_NOT_FOUND,
    );
    registerPaginationCommand(
        bot, CallbackData.TRANSLATOR_BOOKS_PREFIX, getCallbackArgs, getPrefixWithQueryCreator(CallbackData.TRANSLATOR_BOOKS_PREFIX),
        BookLibrary.getTranslatorBooks, formatBookShort, undefined, Messages.BOOKS_NOT_FOUND,
    );
    registerPaginationCommand(
        bot, CallbackData.SEQUENCE_BOOKS_PREFIX, getCallbackArgs, getPrefixWithQueryCreator(CallbackData.SEQUENCE_BOOKS_PREFIX),
        BookLibrary.getSequenceBooks, formatBookShort, undefined, Messages.BOOKS_NOT_FOUND,
    );

    bot.command(["random", `random@${me.username}`], async (ctx: Context) => {
        ctx.reply("Что хотим получить?", {
            reply_markup: getRandomKeyboard().reply_markup,
        })
    });

    registerRandomItemCallback(bot, CallbackData.RANDOM_BOOK, BookLibrary.getRandomBook, formatDetailBook);
    registerRandomItemCallback(bot, CallbackData.RANDOM_AUTHOR, BookLibrary.getRandomAuthor, formatAuthor);
    registerRandomItemCallback(bot, CallbackData.RANDOM_SEQUENCE, BookLibrary.getRandomSequence, formatSequence);

    bot.command(["update_log", `update_log@${me.username}`], async (ctx: Context) => {
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

        const header = `Обновление каталога (${moment(data[2]).format("DD.MM.YYYY")} - ${moment(data[3]).format("DD.MM.YYYY")}):\n\n`;
        const noItemsMessage = 'Нет новых книг за этот период.';

        const pMessage = await getPaginatedMessage(
            `${CallbackData.UPDATE_LOG_PREFIX}${arg}_`, arg, page, allowedLangs, BookLibrary.getBooks, formatBook, header, noItemsMessage,
        );

        try {
            await ctx.editMessageText(pMessage.message, {
                reply_markup: pMessage.keyboard?.reply_markup
            });
        } catch (e) {
            if (!isNotModifiedMessage(e)) {
                Sentry.captureException(e);
            }
        }
    });

    bot.command(["settings", `settings@${me.username}`], async (ctx: Context) => {
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

        try {
            await ctx.editMessageText("Настройки языков:", {
                reply_markup: keyboard.reply_markup,
            });
        } catch (e) {
            if (!isNotModifiedMessage(e)) {
                Sentry.captureException(e);
            }
        }
    });

    registerLanguageSettingsCallback(bot, 'on', CallbackData.ENABLE_LANG_PREFIX);
    registerLanguageSettingsCallback(bot, 'off', CallbackData.DISABLE_LANG_PREFIX);

    bot.hears(new RegExp(`^/d_[a-zA-Z0-9]+_[\\d]+(@${me.username})*$`), async (ctx) => sendFile(ctx, state));

    bot.hears(new RegExp(`^/b_an_[\\d]+(@${me.username})*$`), async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const bookId = ctx.message.text.split("@")[0].split('_')[2];

        const annotation = await BookLibrary.getBookAnnotation(parseInt(bookId));

        if (annotation.file) {
            const imageData = await downloadImage(annotation.file);

            if (imageData) await ctx.telegram.sendPhoto(ctx.message.chat.id, {source: imageData});
        }

        if (!annotation.text) return;

        const data = getTextPaginationData(`${CallbackData.BOOK_ANNOTATION_PREFIX}${bookId}`, annotation.text, 0);

        try {
            await ctx.reply(data.current, {
                parse_mode: "HTML",
                reply_markup: data.keyboard.reply_markup,
            });
        } catch (e) {
            Sentry.captureException(e, {
                extra: {
                    message: data.current,
                }
            })
        }
    });

    bot.action(new RegExp(CallbackData.BOOK_ANNOTATION_PREFIX), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const queryData = ctx.callbackQuery.data.split("_");

        const bookId = queryData[2];
        const page = queryData[3];

        const annotation = await BookLibrary.getBookAnnotation(parseInt(bookId));

        const data = getTextPaginationData(`${CallbackData.BOOK_ANNOTATION_PREFIX}${bookId}`, annotation.text, parseInt(page));

        try {
            await ctx.editMessageText(
                data.current, {
                    parse_mode: "HTML",
                    reply_markup: data.keyboard.reply_markup,
                }
            );
        } catch (e) {
            if (!isNotModifiedMessage(e)) {
                Sentry.captureException(e, {
                    extra: {
                        message: data.current,
                    }
                });
            }
        }
    });

    bot.hears(new RegExp(`^/a_an_[\\d]+(@${me.username})*$`), async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const authorId = ctx.message.text.split('@')[0].split('_')[2];

        const annotation = await BookLibrary.getAuthorAnnotation(parseInt(authorId));

        if (annotation.file) {
            const imageData = await downloadImage(annotation.file);
            
            if (imageData) await ctx.telegram.sendPhoto(ctx.message.chat.id, {source: imageData});
        }

        if (!annotation.text) return;

        const data = getTextPaginationData(`${CallbackData.AUTHOR_ANNOTATION_PREFIX}${authorId}`, annotation.text, 0);
        
        try {
            await ctx.reply(data.current, {
                parse_mode: "HTML",
                reply_markup: data.keyboard.reply_markup,
            });
        } catch (e) {
            Sentry.captureException(e, {
                extra: {
                    message: data.current,
                }
            })
        }
    });

    bot.action(new RegExp(CallbackData.AUTHOR_ANNOTATION_PREFIX), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const queryData = ctx.callbackQuery.data.split("_");

        const authorId = queryData[2];
        const page = queryData[3];

        const annotation = await BookLibrary.getAuthorAnnotation(parseInt(authorId));

        const data = getTextPaginationData(`${CallbackData.AUTHOR_ANNOTATION_PREFIX}${authorId}`, annotation.text, parseInt(page));

        try {
            await ctx.editMessageText(
                data.current, {
                    parse_mode: "HTML",
                    reply_markup: data.keyboard.reply_markup,
                }
            );
        } catch (e) {
            if (!isNotModifiedMessage(e)) {
                Sentry.captureException(e, {
                    extra: {
                        message: data.current,
                    }
                });
            }
        }
    });

    bot.hears(new RegExp(`^/a_[\\d]+(@${me.username})*$`), async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const authorId = ctx.message.text.split('@')[0].split('_')[1];

        const userSettings = await getUserSettings(ctx.message.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const pMessage = await getPaginatedMessage(
            `${CallbackData.AUTHOR_BOOKS_PREFIX}${authorId}_`, parseInt(authorId), 1, 
            allowedLangs, BookLibrary.getAuthorBooks, formatBook, undefined, Messages.BOOKS_NOT_FOUND
        );

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard?.reply_markup
        });
    });

    bot.hears(new RegExp(`^/t_[\\d]+(@${me.username})*$`), async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const translatorId = ctx.message.text.split("@")[0].split('_')[1];

        const userSettings = await getUserSettings(ctx.message.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const pMessage = await getPaginatedMessage(
            `${CallbackData.TRANSLATOR_BOOKS_PREFIX}${translatorId}_`, parseInt(translatorId), 1,
            allowedLangs, BookLibrary.getTranslatorBooks, formatBook, undefined, Messages.BOOKS_NOT_FOUND
        );

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard?.reply_markup
        });
    });

    bot.hears(new RegExp(`^/s_[\\d]+(@${me.username})*$`), async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const sequenceId = ctx.message.text.split("@")[0].split('_')[1];

        const userSettings = await getUserSettings(ctx.message.from.id);
        const allowedLangs = userSettings.allowed_langs.map((lang) => lang.code);

        const pMessage = await getPaginatedMessage(
            `${CallbackData.SEQUENCE_BOOKS_PREFIX}${sequenceId}_`, parseInt(sequenceId), 1, allowedLangs,
            BookLibrary.getSequenceBooks, formatBook, undefined, Messages.BOOKS_NOT_FOUND,
        );

        await ctx.reply(pMessage.message, {
            reply_markup: pMessage.keyboard?.reply_markup
        });
    });

    bot.hears(new RegExp(`^/b_i_[\\d]+(@${me.username})*$`), async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const bookId = ctx.message.text.split("@")[0].split('_')[2];

        const book = await BookLibrary.getBookById(parseInt(bookId));

        await ctx.reply(formatDetailBook(book), {
            reply_to_message_id: ctx.message.message_id,
        });
    });

    bot.on("message", async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        let keyboard = Markup.inlineKeyboard([
            [
                Markup.button.callback('Книгу', `${CallbackData.SEARCH_BOOK_PREFIX}1`)
            ],
            [
                Markup.button.callback('Автора',  `${CallbackData.SEARCH_AUTHORS_PREFIX}1`),
            ],
            [
                Markup.button.callback('Серию', `${CallbackData.SEARCH_SERIES_PREFIX}1`),
            ],
            [
                Markup.button.callback('Переводчика', `${CallbackData.SEARCH_TRANSLATORS_PREFIX}1`),
            ]
        ]);

        try {
            await ctx.telegram.sendMessage(ctx.message.chat.id, Messages.SEARCH_MESSAGE, {
                reply_to_message_id: ctx.message.message_id,
                reply_markup: keyboard.reply_markup,
            });
        } catch (e) {
            if (!isReplyMessageNotFound(e)) {
                Sentry.captureException(e);
            }
        }
    });

    bot.catch((err) => {
        console.log(err);
        Sentry.captureException(err);
    });

    return bot;
}
