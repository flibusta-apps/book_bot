import got from 'got';

import env from '@/config';

import { BotState } from "./types";


export async function makeSyncRequest(): Promise<BotState[] | null> {
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
