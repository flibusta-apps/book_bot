import { Context } from 'telegraf';

import * as BookLibrary from "../services/book_library";
import { clearBookCache, getBookCache } from '../services/book_cache';
import { getBookCacheBuffer } from '../services/book_cache_buffer';
import { download } from '../services/downloader';
import { BotState, Cache } from '@/bots/manager';


async function _sendFile(ctx: Context, state: BotState, chatId: number, id: number, format: string) {
    const sendWithoutCache = async () => {
        const book = await BookLibrary.getBookById(id);
        const data = await download(book.source.id, book.remote_id, format);
        await ctx.telegram.sendDocument(chatId, data)
    }

    const getCachedMessage = async () => {
        if (state.cache === Cache.ORIGINAL) {
            return getBookCache(id, format);
        }

        return getBookCacheBuffer(id, format);
    };

    const sendCached = async () => {
        const cache = await getCachedMessage();
        await ctx.telegram.copyMessage(chatId, cache.chat_id, cache.message_id, {
            allow_sending_without_reply: true,
        });
    };

    if (state.cache === Cache.NO_CACHE) {
        await sendWithoutCache();
        return;
    }

    try {
        await sendCached();
    } catch (e) {
        await clearBookCache(id, format);
        await sendCached();
    }
}


export async function sendFile(ctx: Context, state: BotState) {
    if (!ctx.message || !('text' in ctx.message)) {
        return;
    }

    const [_, format, id] = ctx.message.text.split('_');
    const chatId = ctx.message.chat.id;

    const sendSendingAction = async () => {
        await ctx.telegram.sendChatAction(chatId, "upload_document");
    }

    sendSendingAction();
    const action = setInterval(() => sendSendingAction(), 1000);

    try {
        await _sendFile(ctx, state, chatId, parseInt(id), format);
    } catch (e) {
        await ctx.reply("Ошибка! Попробуйте позже :(", {
            reply_to_message_id: ctx.message.message_id,
        });
    } finally {
        clearInterval(action);
    }
}
