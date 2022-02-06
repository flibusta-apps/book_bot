import got from 'got';

import env from '@/config';


export interface DownloadedFile {
    source: NodeJS.ReadableStream;
    filename: string;
}


export async function download(source_id: number, remote_id: number, file_type: string): Promise<DownloadedFile> {
    const readStream = got.stream.get(`${env.DOWNLOADER_URL}/download/${source_id}/${remote_id}/${file_type}`, {
        headers: {
            'Authorization': env.DOWNLOADER_API_KEY,
        },
    });

    return new Promise<DownloadedFile>((resolve, reject) => {
        readStream.on("response", async response => {
            resolve({
                source: readStream,
                filename: (response.headers['content-disposition'] || '').split('filename=')[1]
            });
        });
    });
}


export async function downloadImage(path: string): Promise<NodeJS.ReadableStream | null> {
    const readStream = got.stream.get(path, {throwHttpErrors: false});

    return new Promise<NodeJS.ReadableStream | null>((resolve, reject) => {
        readStream.on("response", async response => {
            if (response.statusCode === 200) {
                resolve(readStream);
            } else {
                resolve(null);
            }
        });
    
        readStream.once("error", error => {
            resolve(null);
        })
    });
}
