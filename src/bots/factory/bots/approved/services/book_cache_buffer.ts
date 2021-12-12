import got from 'got';

import env from '@/config';
import { CachedMessage } from './book_cache';


async function _makeRequest<T>(url: string, searchParams?: string | Record<string, string | number | boolean | null | undefined> | URLSearchParams | undefined): Promise<T> {
    const response = await got<T>(`${env.BUFFER_SERVER_URL}${url}`, {
        searchParams,
        headers: {
            'Authorization': env.BUFFER_SERVER_API_KEY,
        },
        responseType: 'json',
    });

    return response.body;
}


export async function getBookCacheBuffer(bookId: number, fileType: string): Promise<CachedMessage> {
    return _makeRequest<CachedMessage>(`/api/v1/${bookId}/${fileType}`);
}
