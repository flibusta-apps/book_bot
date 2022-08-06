import { createClient, RedisClientType } from 'redis';

import env from '@/config';

import debug from 'debug';
import Sentry from '@/sentry';


export default class Limiter {
    static debugger = debug("limiter");
    static MAX_PROCESSING_COUNT: number = 3;
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

    static _getKey(updateId: number) {
        return `update_${updateId}`;
    }

    static async _getCount(updateId: number): Promise<number> {
        const key = this._getKey(updateId);
        
        const client = await this._getClient();

        await client.set(key, 0, {EX: 5 * 60, NX: true});
        return client.incr(key);
    }

    static async isLimited(updateId: number): Promise<boolean> {
        const count = await this._getCount(updateId);

        this.debugger(`${updateId}: ${count}`)

        return count > this.MAX_PROCESSING_COUNT;
    }
}
