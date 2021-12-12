import got from 'got';

import env from '@/config';


export interface CachedMessage {
    message_id: number,
    chat_id: string | number,
}


interface BookCache {
    id: number;
    object_id: number;
    object_type: string;
    data: CachedMessage & {
        file_token: string | null,
    }
}


async function _makeRequest<T>(url: string, searchParams?: string | Record<string, string | number | boolean | null | undefined> | URLSearchParams | undefined): Promise<T> {
    const response = await got<T>(`${env.CACHE_SERVER_URL}${url}`, {
        searchParams,
        headers: {
            'Authorization': env.CACHE_SERVER_API_KEY,
        },
        responseType: 'json',
    });

    return response.body;
}


export async function getBookCache(bookId: number, fileType: string): Promise<CachedMessage> {
    return (await _makeRequest<BookCache>(`/api/v1/${bookId}/${fileType}`)).data;
}
