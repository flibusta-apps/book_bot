import { createClient, RedisClientType } from 'redis';
import moment from 'moment';

import env from '@/config';
import BotsManager from '@/bots/manager';

import Sentry from '@/sentry';


enum RedisKeys {
    UsersActivity = "users_activity",
    RequestsCount = "requests_count",
}


export default class UsersCounter {
    static _redisClient: RedisClientType | null = null;
 
    static async _getClient() {
        if (this._redisClient === null) {
            this._redisClient = createClient({
                url: `redis://${env.REDIS_HOST}:${env.REDIS_PORT}/${env.REDIS_DB}`
            });

            this._redisClient.on('error', (err) => {
                console.log(err);
                Sentry.captureException(err);
            });

            await this._redisClient.connect();
        }

        return this._redisClient;
    }

    static async _getBotsUsernames(): Promise<string[]> {        
        const promises = Object.values(BotsManager.bots).map(async (bot) => {
            const botInfo = await bot.telegram.getMe();
            return botInfo.username;
        });

        return Promise.all(promises); 
    }

    static async _getUsersByBot(bot: string): Promise<number[]> {
        const client = await this._getClient();

        return (await client.hKeys(`${RedisKeys.UsersActivity}_${bot}`)).map((userId) => parseInt(userId));
    }

    static async _getAllUsersCount(botsUsernames: string[]): Promise<number> {
        const users = new Set<number>();

        await Promise.all(
            botsUsernames.map(async (bot) => {
                (await this._getUsersByBot(bot)).forEach((user) => users.add(user));
            })
        );

        return users.size;
    }

    static async _getUsersByBots(botsUsernames: string[]): Promise<{[bot: string]: number}> {
        const result: {[bot: string]: number} = {};

        await Promise.all(
            botsUsernames.map(async (bot) => {
                result[bot] = (await this._getUsersByBot(bot)).length;
            })
        );

        return result;
    }

    static async _incrementRequests(bot: string) {
        const client = await this._getClient();

        const key = `${RedisKeys.RequestsCount}_${bot}`;

        const exists = await client.exists(key);

        if (!exists) {
            await client.set(key, 0);
        }

        await client.incr(key);
    }

    static async _getRequestsByBotCount(botsUsernames: string[]): Promise<{[bot: string]: number}> {
        const client = await this._getClient();

        const result: {[bot: string]: number} = {};

        await Promise.all(
            botsUsernames.map(async (bot) => {
                const count = await client.get(`${RedisKeys.RequestsCount}_${bot}`);
                result[bot] = count !== null ? parseInt(count) : 0;
            })
        );

        return result;
    }

    static async take(userId: number, bot: string) {
        const client = await this._getClient();

        await client.hSet(`${RedisKeys.UsersActivity}_${bot}`, userId, moment().format());
        await this._incrementRequests(bot);
    }

    static async getMetrics(): Promise<string> {
        const botUsernames = await this._getBotsUsernames();
        
        const lines = [];

        lines.push(`all_users_count ${await this._getAllUsersCount(botUsernames)}`);

        const requestsByBotCount = await this._getRequestsByBotCount(botUsernames);
        Object.keys(requestsByBotCount).forEach((bot: string) => {
            lines.push(`requests_count{bot="${bot}"} ${requestsByBotCount[bot]}`);
        });

        const usersByBots = await this._getUsersByBots(botUsernames);
        Object.keys(usersByBots).forEach((bot: string) => {
            lines.push(`users_count{bot="${bot}"} ${usersByBots[bot]}`)
        });

        return lines.join("\n");
    }
}
