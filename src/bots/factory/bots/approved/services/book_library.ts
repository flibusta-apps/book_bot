import got from 'got';

import env from '@/config';


const PAGE_SIZE = 7;


export interface Page<T> {
    items: T[];
    page: number;
    size: number;
    total: number;
    total_pages: number;
}


interface BookAuthor {
    id: number;
    first_name: string;
    last_name: string;
    middle_name: string;
}


export interface AuthorBook {
    id: number;
    title: string;
    lang: string;
    file_type: string;
    available_types: string[];
    uploaded: string;
    annotation_exists: boolean;
    translators: BookAuthor[];
}


export interface Book extends AuthorBook {
    authors: BookAuthor[];
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


export async function searchByBookName(query: string, page: number): Promise<Page<Book>> {
    return _makeRequest<Page<Book>>(`/api/v1/books/search/${query}`, {
        page: page,
        size: PAGE_SIZE,
    })
}


export async function searchAuthors(query: string, page: number): Promise<Page<Author>> {
    return _makeRequest<Page<Author>>(`/api/v1/authors/search/${query}`, {
        page: page,
        size: PAGE_SIZE,
    });
}


export async function searchSequences(query: string, page: number): Promise<Page<Sequence>> {
    return _makeRequest<Page<Sequence>>(`/api/v1/sequences/search/${query}`, {
        page: page,
        size: PAGE_SIZE,
    });
}


export async function getBookAnnotation(bookId: number): Promise<BookAnnotation> {
    return _makeRequest<BookAnnotation>(`/api/v1/books/${bookId}/annotation`);
}


export async function getAuthorBooks(authorId: number, page: number): Promise<Page<AuthorBook>> {
    return _makeRequest<Page<AuthorBook>>(`/api/v1/authors/${authorId}/books`, {
        page: page,
        size: PAGE_SIZE,
    });
}


export async function getSequenceBooks(sequenceId: number, page: number): Promise<Page<Book>> {
    return _makeRequest<Page<Book>>(`/api/v1/sequences/${sequenceId}/books`, {
        page: page,
        size: PAGE_SIZE,
    });
}

export async function getRandomBook(): Promise<Book> {
    return _makeRequest<Book>('/api/v1/books/random');
}

export async function getRandomAuthor(): Promise<Author> {
    return _makeRequest<Author>('/api/v1/authors/random');
}

export async function getRandomSequence(): Promise<Sequence> {
    return _makeRequest<Sequence>('/api/v1/sequences/random');
}
