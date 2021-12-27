import express, { Response, Request, NextFunction } from 'express';

import got from 'got';

import { Telegraf } from 'telegraf';

import env from '@/config';
import getBot, { BotStatuses } from './factory/index';
import { Server } from 'http';

export enum Cache {
    ORIGINAL = "original",
    BUFFER = "buffer",
    NO_CACHE = "no_cache"
}

export interface BotState {
    id: number;
    token: string;
    status: BotStatuses;
    cache: Cache;
    created_time: string;
}


async function _makeSyncRequest(): Promise<BotState[] | null> {
    try {
        const response = await got<BotState[]>(env.MANAGER_URL, {
            headers: {
                'Authorization': env.MANAGER_API_KEY
            },
            responseType: 'json',
        });

        return response.body;
    } catch (err) {
        return null;
    }
}


export default class BotsManager {
    static bots: {[key: number]: Telegraf} = {};
    static botsStates: {[key: number]: BotStatuses} = {};
    static syncInterval: NodeJS.Timer | null = null;
    static server: Server | null = null;

    static async start() {
        await this.sync();

        this.launch();

        await this.sync();
        if (this.syncInterval === null) {
            this.syncInterval = setInterval(() => this.sync(), 30_000);
        }
    }

    static async sync() {
        const botsData = await _makeSyncRequest();

        if (botsData !== null) {
            await Promise.all(botsData.map((state) => this.updateBotState(state)));
        }
    }

    static async updateBotState(state: BotState) {
        const isExists = this.bots[state.id] !== undefined;

        if (isExists && this.botsStates[state.id] === state.status) {
            return;
        }

        const bot = await getBot(state.token, state);

        this.bots[state.id] = bot;
        this.botsStates[state.id] = state.status;

        try {
            const oldBot = new Telegraf(bot.telegram.token);
            await oldBot.telegram.deleteWebhook();
            await oldBot.telegram.logOut();
        } catch (e) {
            console.log(e);
        }

        await bot.telegram.setWebhook(
            `${env.WEBHOOK_BASE_URL}:${env.WEBHOOK_PORT}/${state.id}/${bot.telegram.token}`
        );
    }

    static async handleUpdate(req: Request, res: Response, next: NextFunction) {
        const botIdStr = req.url.split("/")[1];
        const bot = this.bots[parseInt(botIdStr)];
        await bot.webhookCallback(`/${botIdStr}/${bot.telegram.token}`)(req, res);
    }

    static async launch() {
        const application = express();
        application.use((req: Request, res: Response, next: NextFunction) => this.handleUpdate(req, res, next));
        this.server = application.listen(env.WEBHOOK_PORT);
        console.log("Server started!");

        process.once('SIGINT', () => this.stop());
        process.once('SIGTERM', () => this.stop());
    }

    static stop() {
        Object.keys(this.bots).forEach(key => this.bots[parseInt(key)].telegram.deleteWebhook());

        if (this.syncInterval) {
            clearInterval(this.syncInterval);
            this.syncInterval = null;
        }

        this.server?.close();
        this.server = null;
    }
}
