import { Markup } from 'telegraf';
import { InlineKeyboardMarkup } from 'typegram';
import moment from 'moment';

import { RANDOM_BOOK, RANDOM_AUTHOR, RANDOM_SEQUENCE, ENABLE_LANG_PREFIX, DISABLE_LANG_PREFIX, UPDATE_LOG_PREFIX } from './callback_data';
import { getUserSettings, getLanguages } from './services/user_settings';


function getButtonLabel(delta: number, direction: 'left' | 'right'): string {
    if (delta == 1) {
        return direction === 'left' ? "<" : ">";
    }

    return direction === 'left' ? `< ${delta} <` : `> ${delta} >`;
}


export function getPaginationKeyboard(prefix: string, query: string | number, page: number, totalPages: number): Markup.Markup<InlineKeyboardMarkup> {
    function getRow(delta: number) {
        const row = [];

        if (page - delta > 0) {
            row.push(Markup.button.callback(getButtonLabel(delta, 'left'), `${prefix}${query}_${page - delta}`));
        }
        if (page + delta <= totalPages) {
            row.push(Markup.button.callback(getButtonLabel(delta, 'right'), `${prefix}${query}_${page + delta}`));
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


export function getUpdateLogKeyboard(): Markup.Markup<InlineKeyboardMarkup> {
    const format = "YYYY-MM-DD";

    const now = moment().format(format);
    const d3 = moment().subtract(3, 'days').format(format);
    const d7 = moment().subtract(7, 'days').format(format);
    const d30 = moment().subtract(30, 'days').format(format);

    return Markup.inlineKeyboard([
        [Markup.button.callback('За 3 дня', `${UPDATE_LOG_PREFIX}${d3}_${now}_1`)],
        [Markup.button.callback('За 7 дней', `${UPDATE_LOG_PREFIX}${d7}_${now}_1`)],
        [Markup.button.callback('За 30 дней', `${UPDATE_LOG_PREFIX}${d30}_${now}_1`)],
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
