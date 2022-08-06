import { Context, Telegraf, Markup, TelegramError } from 'telegraf';
import moment from 'moment';

import { BotState } from '@/bots/manager/types';

import env from '@/config';

import * as Messages from "./messages";

import * as CallbackData from "./callback_data";

import * as BookLibrary from "./services/book_library";
import * as Rating from "./services/book_ratings";
import UsersCounter from '@/analytics/users_counter';
import { createOrUpdateUserSettings, getUserOrDefaultLangCodes } from './services/user_settings';
import { formatBook, formatBookShort, formatAuthor, formatSequence, formatTranslator, formatDetailBook, formatDetailBookWithRating } from './format';
import { getCallbackArgs, getPaginatedMessage, getPrefixWithQueryCreator, getSearchArgs, registerLanguageSettingsCallback, registerPaginationCommand, registerRandomItemCallback } from './utils';
import { getRandomKeyboard, getRatingKeyboard, getTextPaginationData, getUpdateLogKeyboard, getUserAllowedLangsKeyboard } from './keyboard';
import { sendFile } from './hooks/downloading';
import { setCommands } from './hooks/setCommands';
import { isNotModifiedMessage, isReplyMessageNotFound } from './errors_utils';
import { getAnnotationHandler } from './annotations';
import Sentry from '@/sentry';


export async function createApprovedBot(token: string, state: BotState): Promise<Telegraf> {
    const bot = new Telegraf(token, {
        telegram: {
            apiRoot: env.TELEGRAM_BOT_API_ROOT,
        },
        handlerTimeout: 300_000,
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
            }).catch((e) => {
                Sentry.captureException(e);
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
        
        try {
            await ctx.telegram.sendMessage(ctx.message.chat.id,
                Messages.START_MESSAGE.replace('{name}', name), {
                    reply_to_message_id: ctx.message.message_id,
                }
            );
        } catch (e) {
            if (e instanceof TelegramError) {
                if (e.code !== 403) throw e;
            }
        }
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

    bot.action(CallbackData.RANDOM_BOOK_BY_GENRE_REQUEST, async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const metaGenres = await BookLibrary.getGenreMetas();

        const keyboard = Markup.inlineKeyboard(
            metaGenres.map((meta, index) => {
                return [Markup.button.callback(meta, `${CallbackData.GENRES_PREFIX}${index}`)];
            })
        );

        await ctx.editMessageReplyMarkup(keyboard.reply_markup);
    });

    bot.action(new RegExp(CallbackData.GENRES_PREFIX), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const queryData = ctx.callbackQuery.data.split("_");
        const metaIndex = parseInt(queryData[1]);

        const metaGenres = await BookLibrary.getGenreMetas();
        const meta = metaGenres[metaIndex];

        const genres = await BookLibrary.getGenres(meta);

        const buttons = genres.items.map((genre) => {
            return [Markup.button.callback(genre.description, `${CallbackData.RANDOM_BOOK_BY_GENRE}${genre.id}`)]
        });
        buttons.push(
            [Markup.button.callback("< Назад >", CallbackData.RANDOM_BOOK_BY_GENRE_REQUEST)]
        );

        const keyboard = Markup.inlineKeyboard(buttons);

        await ctx.editMessageReplyMarkup(keyboard.reply_markup);
    });

    bot.action(new RegExp(CallbackData.RANDOM_BOOK_BY_GENRE), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const allowedLangs = await getUserOrDefaultLangCodes(ctx.callbackQuery.from.id);
        
        const queryData = ctx.callbackQuery.data.split("_");
        const genreId = parseInt(queryData[4]);

        const item = await BookLibrary.getRandomBook(allowedLangs, genreId);
        const keyboard = Markup.inlineKeyboard([
            [Markup.button.callback("Повторить?", ctx.callbackQuery.data)]
        ]);

        try {
            await ctx.editMessageReplyMarkup(Markup.inlineKeyboard([]).reply_markup);
        } catch (e) {}

        ctx.reply(formatDetailBook(item), {
            reply_markup: keyboard.reply_markup,
        });
    });

    bot.command(["update_log", `update_log@${me.username}`], async (ctx: Context) => {
        ctx.reply("Обновление каталога: ", {
            reply_markup: getUpdateLogKeyboard().reply_markup,
        });
    });

    bot.action(new RegExp(CallbackData.UPDATE_LOG_PREFIX), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const allowedLangs = await getUserOrDefaultLangCodes(ctx.callbackQuery.from.id);

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

    bot.hears(new RegExp(`^/d_[a-zA-Z0-9]+_[\\d]+(@${me.username})*$`), async (ctx) => {
        try {
            await sendFile(ctx, state)
        } catch (e) {
            Sentry.captureException(e, {
                extra: {
                    action: "sendFile",
                    message: ctx.message.text,
                }
            })
        }
    });

    bot.hears(
        new RegExp(`^/b_an_[\\d]+(@${me.username})*$`),
        getAnnotationHandler(BookLibrary.getBookAnnotation, CallbackData.BOOK_ANNOTATION_PREFIX)
    );

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
                        bookId,
                        page,
                    }
                });
            }
        }
    });

    bot.hears(
        new RegExp(`^/a_an_[\\d]+(@${me.username})*$`),
        getAnnotationHandler(BookLibrary.getAuthorAnnotation, CallbackData.AUTHOR_ANNOTATION_PREFIX)
    );

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
                        authorId,
                        page,
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

        const allowedLangs = await getUserOrDefaultLangCodes(ctx.message.from.id);

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

        const allowedLangs = await getUserOrDefaultLangCodes(ctx.message.from.id);

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

        const allowedLangs = await getUserOrDefaultLangCodes(ctx.message.from.id);

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

        const bookIdString = ctx.message.text.split("@")[0].split('_')[2];
        const bookId = parseInt(bookIdString);

        const book = await BookLibrary.getBookById(bookId);
        const keyboard = await getRatingKeyboard(ctx.message.from.id, bookId, null);

        await ctx.reply(formatDetailBookWithRating(book), {
            reply_to_message_id: ctx.message.message_id,
            reply_markup: keyboard.reply_markup,
        });
    });

    bot.action(new RegExp(CallbackData.RATE_PREFIX), async (ctx: Context) => {
        if (!ctx.callbackQuery || !('data' in ctx.callbackQuery)) return;

        const queryData = ctx.callbackQuery.data.split("_");

        const userId = parseInt(queryData[1]);
        const bookId = parseInt(queryData[2]);
        const rate = parseInt(queryData[3]);

        const rating = await Rating.set(userId, bookId, rate);

        const keyboard = await getRatingKeyboard(userId, bookId, rating);

        try {
            await ctx.editMessageReplyMarkup(
                keyboard.reply_markup
            );
        } catch (e) {}
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

    bot.catch((err, ctx: Context) => {
        console.log(err, ctx);
        Sentry.captureException(err);
    });

    return bot;
}
