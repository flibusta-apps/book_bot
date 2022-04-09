import got from 'got';

import env from '@/config';


export interface Rating {
    id: number;
    user_id: number;
    book_id: number;
    rate: number;
    updated: string;
}


export async function get(userId: number, bookId: number): Promise<Rating | null> {
    try {
        const response = await got<Rating>(`${env.RATINGS_URL}/api/v1/ratings/${userId}/${bookId}`, {
            headers: {
                'Authorization': env.RATINGS_API_KEY,
            },
            responseType: 'json',
        });

        return response.body;
    } catch {
        return null;
    }
}


export async function set(userId: number, bookId: number, rate: number): Promise<Rating> {
    const response = await got.post<Rating>(`${env.RATINGS_URL}/api/v1/ratings`, {
        json: {
            "user_id": userId,
            "book_id": bookId,
            "rate": rate,
        },
        headers: {
            'Authorization': env.RATINGS_API_KEY,
            'Content-Type': 'application/json',
        },
        responseType: 'json'
    });

    return response.body;
}
