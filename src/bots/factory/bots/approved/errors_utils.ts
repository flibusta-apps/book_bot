import { TelegramError } from 'telegraf';


export function isNotModifiedMessage(e: any): boolean {
    if (!(e instanceof TelegramError)) return false;

    return e.description === 'Bad Request: message is not modified: specified new message content and reply markup are exactly the same as a current content and reply markup of the message';
}
