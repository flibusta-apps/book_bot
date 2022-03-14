import * as Sentry from '@sentry/node';

import express, { Response, Request, NextFunction } from 'express';
import { Server } from 'http';

import * as dockerIpTools from "docker-ip-get";

import { Telegraf } from 'telegraf';

import env from '@/config';
import getBot from '../factory/index';
import UsersCounter from '@/analytics/users_counter';
import { makeSyncRequest } from "./utils";
import { BotState } from "./types";


Sentry.init({
    dsn: env.SENTRY_DSN,
});


export default class BotsManager {
    static server: Server | null = null;
    
    // Bots
    static bots: {[key: number]: Telegraf} = {};
    static botsStates: {[key: number]: BotState} = {};
    static botsPendingUpdatesCount: {[key: number]: number} = {};

    // Intervals
    static syncInterval: NodeJS.Timer | null = null;

    static async start() {
        this.launch();

        await this.sync();

        if (this.syncInterval === null) {
            this.syncInterval = setInterval(() => this.sync(), 30_000);
        }
    }

    static async sync() {
        const botsData = await makeSyncRequest();

        if (botsData !== null) {
            await Promise.all(botsData.map((state) => this._updateBotState(state)));
        }

        if (botsData !== null) {
            await Promise.all(
                Object.values(this.botsStates).map(
                    (value: BotState) => this._checkPendingUpdates(this.bots[value.id], value)
                )
            );

            console.log("Bots pending updates count:", this.botsPendingUpdatesCount);
        }
    }
    
    static async _updateBotState(state: BotState) {
        const isExists = this.bots[state.id] !== undefined;

        if (isExists &&
            this.botsStates[state.id].status === state.status &&
            this.botsStates[state.id].cache === state.cache
        ) {
            return;
        }

        try {
            const oldBot = new Telegraf(state.token);
            await oldBot.telegram.deleteWebhook();
            await oldBot.telegram.logOut();
        } catch (e) {}

        let bot: Telegraf;

        try {
            bot = await getBot(state.token, state);
        } catch (e) {
            return;
        }

        if (!(await this._setWebhook(bot, state))) return;

        this.bots[state.id] = bot;
        this.botsStates[state.id] = state;
    }

    static async _checkPendingUpdates(bot: Telegraf, state: BotState) {
        try {
            const webhookInfo = await bot.telegram.getWebhookInfo();
            const previousPendingUpdateCount = this.botsPendingUpdatesCount[state.id] || 0;

            if (previousPendingUpdateCount !== 0 && webhookInfo.pending_update_count !== 0) {
                this._setWebhook(bot, state);
            }

            this.botsPendingUpdatesCount[state.id] = webhookInfo.pending_update_count;
        } catch (e) {
            Sentry.captureException(e, {
                extra: {
                    method: "_checkPendingUpdate",
                    state_id: state.id,
                }
            });
        }
    }

    static async _setWebhook(bot: Telegraf, state: BotState): Promise<boolean> {
        const dockerIps = (await dockerIpTools.getContainerIp()).split(" ");
        const filteredIp = dockerIps.filter((ip) => ip.startsWith(env.NETWORK_IP_PREFIX));

        const ips = filteredIp.length !== 0 ? filteredIp : dockerIps;

        for (const ip of ips) {
            try {
                await bot.telegram.setWebhook(
                    `${env.WEBHOOK_BASE_URL}:${env.WEBHOOK_PORT}/${state.id}/${bot.telegram.token}`, {
                        ip_address: ip,
                    }
                );
                return true;
            } catch (e) {
                console.log(e);
            }
        }
        return false;
    }

    static async handleUpdate(req: Request, res: Response, next: NextFunction) {
        const botIdStr = req.url.split("/")[1];
        const bot = this.bots[parseInt(botIdStr)];
        await bot.webhookCallback(`/${botIdStr}/${bot.telegram.token}`)(req, res);
    }

    private static async launch() {
        const application = express();

        application.get("/healthcheck", (req, res) => {
            res.send("Ok!");
        });

        application.get("/metrics", (req, res) => {
            res.send(UsersCounter.getMetrics());
        });

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
