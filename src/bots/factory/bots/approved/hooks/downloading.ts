import { Context } from 'telegraf';

import { clearBookCache, getBookCache, downloadFromCache } from '../services/book_cache';
import { getBookCacheBuffer } from '../services/book_cache_buffer';
import { BotState, Cache } from '@/bots/manager';


async function _sendFile(ctx: Context, state: BotState, chatId: number, id: number, format: string) {
    const sendWithDownloadFromChannel = async () => {
        const data = await downloadFromCache(id, format);
        await ctx.telegram.sendDocument(chatId, { source: data.source, filename: data.filename }, { caption: data.caption });
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
        return sendWithDownloadFromChannel();
    }

    try {
        return await sendCached();
    } catch (e) {
        await clearBookCache(id, format);
        return sendCached();
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
        return await _sendFile(ctx, state, chatId, parseInt(id), format);
    } catch (e) {
        console.log(e);

        return await ctx.reply("Ошибка! Попробуйте позже :(", {
            reply_to_message_id: ctx.message.message_id,
        });
    } finally {
        clearInterval(action);
    }
}
