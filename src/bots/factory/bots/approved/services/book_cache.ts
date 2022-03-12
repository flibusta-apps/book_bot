import got, { Response } from 'got';
import { decode } from 'js-base64';

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
    source: NodeJS.ReadableStream;
    filename: string;
    caption: string;
}

export async function downloadFromCache(bookId: number, fileType: string): Promise<DownloadedFile | null> {
    const readStream = got.stream.get(`${env.CACHE_SERVER_URL}/api/v1/download/${bookId}/${fileType}`, {
        headers: {
            'Authorization': env.CACHE_SERVER_API_KEY,
        },
    });

    return new Promise<DownloadedFile | null>((resolve, reject) => {
        let timeout: NodeJS.Timeout | null = null;

        const resolver = async (response: Response) => {
            if (response.statusCode !== 200) {
                resolve(null);
                if (timeout) clearTimeout(timeout);
                return
            }

            const captionData = response.headers['x-caption-b64'];

            if (captionData === undefined || Array.isArray(captionData)) throw Error('No caption?');

            if (timeout) clearTimeout(timeout);

            return resolve({
                source: readStream,
                filename: (response.headers['content-disposition'] || '').replaceAll('"', "").split('filename=')[1],
                caption: decode(captionData),
            })
        }

        timeout = setTimeout(() => {
            readStream.off("response", resolver);
            resolve(null);
        }, 60_000);
        
        readStream.on("response", resolver);
    });
}
