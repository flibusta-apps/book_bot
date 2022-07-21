import got from 'got';

import env from '@/config';
import { getAllowedLangsSearchParams } from '../utils';


const PAGE_SIZE = 7;


export interface Page<T> {
    items: T[];
    page: number;
    size: number;
    total: number;
    total_pages: number;
}


export interface BookAuthor {
    id: number;
    first_name: string;
    last_name: string;
    middle_name: string;
}


export interface BaseBook {
    id: number;
    title: string;
    lang: string;
    file_type: string;
    available_types: string[];
    uploaded: string;
    annotation_exists: boolean;
}


export interface AuthorBook extends BaseBook {
    translators: BookAuthor[];
}


export interface TranslatorBook extends BaseBook {
    authors: BookAuthor[];
}


export interface Book extends BaseBook {
    authors: BookAuthor[];
    translators: BookAuthor[];
}


export interface Genre {
    id: number;
    description: string;
}


export interface Source {
    id: number;
    name: string;
}


export interface DetailBook extends Book {
    sequences: Sequence[];
    genres: Genre[];
    source: Source;
    remote_id: number;
    is_deleted: boolean;
    pages: number | null;
}


export interface Author {
    id: number;
    last_name: string;
    first_name: string;
    middle_name: string;
    annotation_exists: boolean;
}


export interface Sequence {
    id: number;
    name: string;
}


export interface AuthorAnnnotation {
    id: number;
    title: string;
    text: string;
    file: string | null;
}


export interface BookAnnotation {
    id: number;
    title: string;
    text: string;
    file: string | null;
}


async function _makeRequest<T>(url: string, searchParams?: string | Record<string, string | number | boolean | null | undefined> | URLSearchParams | undefined): Promise<T> {
    const response = await got<T>(`${env.BOOK_SERVER_URL}${url}`, {
        searchParams,
        headers: {
            'Authorization': env.BOOK_SERVER_API_KEY,
        },
        responseType: 'json',
    });

    return response.body;
}


export async function getBooks(query: string, page: number, allowedLangs: string[]): Promise<Page<Book>> {
    const queryDates = query.split("_");

    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());
    searchParams.append('uploaded_gte', queryDates[0]);
    searchParams.append('uploaded_lte', queryDates[1]);
    searchParams.append('is_deleted', 'false');

    return _makeRequest<Page<Book>>(`/api/v1/books/`, searchParams);
}


export async function getBookById(book_id: number): Promise<DetailBook> {
    return _makeRequest<DetailBook>(`/api/v1/books/${book_id}`);
}


export async function searchByBookName(query: string, page: number, allowedLangs: string[]): Promise<Page<Book>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<Book>>(`/api/v1/books/search/${query}`, searchParams);
}


export async function searchAuthors(query: string, page: number, allowedLangs: string[]): Promise<Page<Author>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<Author>>(`/api/v1/authors/search/${query}`, searchParams);
}


export async function searchTranslators(query: string, page: number, allowedLangs: string[]): Promise<Page<Author>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<Author>>(`/api/v1/translators/search/${query}`, searchParams);
}


export async function searchSequences(query: string, page: number, allowedLangs: string[]): Promise<Page<Sequence>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<Sequence>>(`/api/v1/sequences/search/${query}`, searchParams);
}


export async function getBookAnnotation(bookId: number): Promise<BookAnnotation> {
    return _makeRequest<BookAnnotation>(`/api/v1/books/${bookId}/annotation`);
}


export async function getAuthorAnnotation(authorId: number): Promise<AuthorAnnnotation> {
    return _makeRequest<AuthorAnnnotation>(`/api/v1/authors/${authorId}/annotation`);
}


export async function getAuthorBooks(authorId: number | string, page: number, allowedLangs: string[]): Promise<Page<AuthorBook>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<AuthorBook>>(`/api/v1/authors/${authorId}/books`, searchParams);
}


export async function getTranslatorBooks(translatorId: number | string, page: number, allowedLangs: string[]): Promise<Page<AuthorBook>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<AuthorBook>>(`/api/v1/translators/${translatorId}/books`, searchParams);
}


export async function getSequenceBooks(sequenceId: number | string, page: number, allowedLangs: string[]): Promise<Page<Book>> {
    const searchParams = getAllowedLangsSearchParams(allowedLangs);
    searchParams.append('page', page.toString());
    searchParams.append('size', PAGE_SIZE.toString());

    return _makeRequest<Page<Book>>(`/api/v1/sequences/${sequenceId}/books`, searchParams);
}

export async function getRandomBook(allowedLangs: string[], genre: number | null = null): Promise<DetailBook> {
    const params = getAllowedLangsSearchParams(allowedLangs);
    if (genre) params.append("genre", genre.toString());

    return _makeRequest<DetailBook>(
        '/api/v1/books/random',
        params,
    );
}

export async function getRandomAuthor(allowedLangs: string[]): Promise<Author> {
    return _makeRequest<Author>('/api/v1/authors/random', getAllowedLangsSearchParams(allowedLangs));
}

export async function getRandomSequence(allowedLangs: string[]): Promise<Sequence> {
    return _makeRequest<Sequence>('/api/v1/sequences/random', getAllowedLangsSearchParams(allowedLangs));
}

export async function getGenreMetas(): Promise<string[]> {
    return _makeRequest<string[]>('/api/v1/genres/metas');
}

export async function getGenres(meta: string): Promise<Page<Genre>> {
    return _makeRequest<Page<Genre>>('/api/v1/genres', {meta});
}
