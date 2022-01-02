import got from 'got';

import env from '@/config';


interface Language {
    id: number;
    label: string;
    code: string;
}


interface UserSettings {
    user_id: number;
    last_name: string;
    first_name: string;
    source: string;
    allowed_langs: Language[];
}


export interface UserSettingsUpdateData {
    user_id: number;
    last_name: string;
    first_name: string;
    username: string;
    source: string;
}


async function _makeGetRequest<T>(url: string, searchParams?: string | Record<string, string | number | boolean | null | undefined> | URLSearchParams | undefined): Promise<T> {
    const response = await got<T>(`${env.USER_SETTINGS_URL}${url}`, {
        searchParams,
        headers: {
            'Authorization': env.USER_SETTINGS_API_KEY,
        },
        responseType: 'json'
    });

    return response.body;
}


export async function getLanguages(): Promise<Language[]> {
    return _makeGetRequest<Language[]>('/languages');
}


export async function getUserSettings(user_id: number): Promise<UserSettings | null> {
    return _makeGetRequest<UserSettings>(`/users/${user_id}`);
}

export async function createOrUpdateUserSettings(data: UserSettingsUpdateData): Promise<UserSettings> {
    const response = await got.post<UserSettings>(`${env.USER_SETTINGS_URL}/users/`, {
        json: data,
        headers: {
            'Authorization': env.USER_SETTINGS_API_KEY,
            'Content-Type': 'application/json',
        },
        responseType: 'json'
    });

    return response.body;
}
