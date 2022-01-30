import got from 'got';

import env from '@/config';


export interface DownloadedFile {
    source: Buffer;
    filename: string;
}


export async function download(source_id: number, remote_id: number, file_type: string): Promise<DownloadedFile> {
    const response = await got<string>(`${env.DOWNLOADER_URL}/download/${source_id}/${remote_id}/${file_type}`, {
        headers: {
            'Authorization': env.DOWNLOADER_API_KEY,
        },
    });

    return {
        source: response.rawBody,
        filename: (response.headers['content-disposition'] || '').split('filename=')[1]
    }
}


export async function downloadImage(path: string) {
    const response  = await got(path);

    if (response.statusCode === 200) {
        return response.rawBody;
    } else {
        return null;
    }
}
