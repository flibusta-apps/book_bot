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
    allowed_langs?: string[];
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


export async function getUserSettings(userId: number): Promise<UserSettings> {
    return _makeGetRequest<UserSettings>(`/users/${userId}`);
}


export async function getUserOrDefaultLangCodes(userId: number): Promise<string[]> {
    try {
        return (await getUserSettings(userId)).allowed_langs.map((lang) => lang.code);
    } catch {
        return ["ru", "be", "uk"];
    }
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
