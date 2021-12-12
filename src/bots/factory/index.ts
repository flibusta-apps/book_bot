import { Telegraf } from "telegraf";

import { BotState } from '@/bots/manager';

import { createPendingBot } from './bots/pending';
import { createBlockedBot } from './bots/blocked';
import { createApprovedBot } from './bots/approved/index';


export enum BotStatuses {
    PENDING = 'pending',
    APPROVED = 'approved',
    BLOCKED = 'blocked',
}


export default async function getBot(token: string, state: BotState): Promise<Telegraf> {
    const handlers = {
        [BotStatuses.PENDING]: createPendingBot,
        [BotStatuses.BLOCKED]: createBlockedBot,
        [BotStatuses.APPROVED]: createApprovedBot,
    };

    return handlers[state.status](token, state);
}
