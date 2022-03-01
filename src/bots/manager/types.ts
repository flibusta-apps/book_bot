import { BotStatuses } from '../factory/index';


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
