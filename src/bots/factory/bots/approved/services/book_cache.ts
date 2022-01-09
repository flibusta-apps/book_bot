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

async function _makeDeleteRequest<T>(url: string, searchParams?: string | Record<string, string | number | boolean | null | undefined> | URLSearchParams | undefined): Promise<T> {
    const response = await got.delete<T>(`${env.CACHE_SERVER_URL}${url}`, {
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

export async function clearBookCache(bookId: number, fileType: string): Promise<CachedMessage> {
    return (await _makeDeleteRequest<BookCache>(`/api/v1/${bookId}/${fileType}`)).data;
}

export interface DownloadedFile {
    source: Buffer;
    filename: string;
}

export async function downloadFromCache(bookId: number, fileType: string): Promise<DownloadedFile> {
    const response = await got<string>(`${env.DOWNLOADER_URL}/api/v1/download/${bookId}/${fileType}`, {
        headers: {
            'Authorization': env.DOWNLOADER_API_KEY,
        },
    });

    return {
        source: response.rawBody,
        filename: (response.headers['content-disposition'] || '').split('filename=')[1]
    }
}
