import { Markup } from 'telegraf';
import { InlineKeyboardMarkup } from 'typegram';

import { RANDOM_BOOK, RANDOM_AUTHOR, RANDOM_SEQUENCE, ENABLE_LANG_PREFIX, DISABLE_LANG_PREFIX } from './callback_data';
import { getUserSettings, getLanguages } from './services/user_settings';


export function getPaginationKeyboard(prefix: string, query: string, page: number, totalPages: number): Markup.Markup<InlineKeyboardMarkup> {
    function getRow(delta: number) {
        const row = [];

        if (page - delta > 0) {
            row.push(Markup.button.callback(`-${delta}`, `${prefix}${query}_${page - delta}`));
        }
        if (page + delta <= totalPages) {
            row.push(Markup.button.callback(`+${delta}`, `${prefix}${query}_${page + delta}`));
        }

        return row;
    }

    const rows = [];

    const row1 = getRow(1);
    if (row1) {
        rows.push(row1);
    }

    const row5 = getRow(5);
    if (row5) {
        rows.push(row5);
    }

    return Markup.inlineKeyboard(rows);
}


export function getRandomKeyboard(): Markup.Markup<InlineKeyboardMarkup> {
    return Markup.inlineKeyboard([
        [Markup.button.callback('Книгу', RANDOM_BOOK)],
        [Markup.button.callback('Автора', RANDOM_AUTHOR)],
        [Markup.button.callback('Серию', RANDOM_SEQUENCE)],
    ]);
}

const DEFAULT_ALLOWED_LANGS_CODES = ['ru', 'be', 'uk'];

export async function getUserAllowedLangsKeyboard(userId: number): Promise<Markup.Markup<InlineKeyboardMarkup>> {
    const allLangs = await getLanguages();
    const userSettings = await getUserSettings(userId);

    const userAllowedLangsCodes = userSettings !== null 
        ? userSettings.allowed_langs.map((lang) => lang.code)
        : DEFAULT_ALLOWED_LANGS_CODES;

    const onEmoji = '🟢';
    const offEmoji = '🔴';

    return Markup.inlineKeyboard([
        ...allLangs.map((lang) => {
            let titlePrefix: string;
            let callbackDataPrefix: string;
            if (userAllowedLangsCodes.includes(lang.code)) {
                titlePrefix = onEmoji;
                callbackDataPrefix = DISABLE_LANG_PREFIX;
            } else {
                titlePrefix = offEmoji;
                callbackDataPrefix = ENABLE_LANG_PREFIX;
            }
            const title = `${titlePrefix} ${lang.label}`;
            return [Markup.button.callback(title, `${callbackDataPrefix}${lang.code}`)];
        })
    ]);
}
