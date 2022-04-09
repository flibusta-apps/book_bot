import { AuthorBook, TranslatorBook, Book, Author, Sequence, BookAuthor, DetailBook, Genre } from './services/book_library';


type AllBookTypes = Book | AuthorBook | TranslatorBook;


function isAuthorBook(item: AllBookTypes): item is AuthorBook {
    return 'translator' in item;
}


function isTranslatorBook(item: AllBookTypes): item is TranslatorBook {
    return 'authors' in item;
}


export function formatBook(book: AllBookTypes, short: boolean = false): string {
    let response: string[] = [];

    response.push(`📖 ${book.title} | ${book.lang}`);

    response.push(`Информация: /b_i_${book.id}`);
    
    const pushAuthorOrTranslator = (author: BookAuthor) => response.push(
        `͏👤 ${author.last_name} ${author.first_name} ${author.middle_name}`
    );

    if (isTranslatorBook(book) && book.authors.length > 0) {
        response.push('Авторы:')

        if (short && book.authors.length >= 5) {
            book.authors.slice(0, 5).forEach(pushAuthorOrTranslator);
            response.push("  и другие.");
        } else {
            book.authors.forEach(pushAuthorOrTranslator);
        }
    }

    if (isAuthorBook(book) && book.translators.length > 0) {
        response.push('Переводчики:');

        if (short && book.translators.length >= 5) {
            book.translators.slice(0, 5).forEach(pushAuthorOrTranslator);
            response.push("  и другие.")
        } else {
            book.translators.forEach(pushAuthorOrTranslator);
        }
    }

    book.available_types.forEach(a_type => response.push(`📥 ${a_type}: /d_${a_type}_${book.id}`));

    return response.join('\n');
}

export function formatDetailBook(book: DetailBook): string {
    let response: string[] = [];

    const addEmptyLine = () => response.push("");

    response.push(`📖 ${book.title} | ${book.lang}`);
    addEmptyLine();

    if (book.annotation_exists) {
        response.push(`📝 Аннотация: /b_an_${book.id}`)
        addEmptyLine();
    }

    if (book.authors.length > 0) {
        response.push('Авторы:')

        const pushAuthor = (author: BookAuthor) => response.push(
            `͏👤 ${author.last_name} ${author.first_name} ${author.middle_name} /a_${author.id}`
        );
        book.authors.forEach(pushAuthor);
        addEmptyLine();
    }

    if (book.translators.length > 0) {
        response.push('Переводчики:');

        const pushTranslator = (author: BookAuthor) => response.push(
            `͏👤 ${author.last_name} ${author.first_name} ${author.middle_name} /t_${author.id}`
        );
        book.translators.forEach(pushTranslator);
        addEmptyLine();
    }

    if (book.sequences.length > 0) {
        response.push('Серии:');

        const pushSequence = (sequence: Sequence) => response.push(
            `📚 ${sequence.name} /s_${sequence.id}`
        );
        book.sequences.forEach(pushSequence);
        addEmptyLine();
    }

    if (book.genres.length > 0) {
        response.push('Жанры:');

        const pushGenre = (genre: Genre) => response.push(
            `🗂 ${genre.description}`
        );
        book.genres.forEach(pushGenre);
        addEmptyLine();
    }

    response.push("Скачать: ")
    book.available_types.forEach(a_type => response.push(`📥 ${a_type}: /d_${a_type}_${book.id}`));

    return response.join('\n');
}


export function formatDetailBookWithRating(book: DetailBook): string {
    return formatDetailBook(book) + '\n\n\nОценка:';
}


export function formatBookShort(book: AllBookTypes): string {
    return formatBook(book, true);
}


export function formatAuthor(author: Author): string {
    let response = [];

    response.push(`👤 ${author.last_name} ${author.first_name} ${author.middle_name}`);
    response.push(`/a_${author.id}`);

    if (author.annotation_exists) {
        response.push(`📝 Аннотация: /a_an_${author.id}`);
    }

    return response.join('\n');
}


export function formatTranslator(author: Author): string {
    let response = [];

    response.push(`👤 ${author.last_name} ${author.first_name} ${author.middle_name}`);
    response.push(`/t_${author.id}`);

    if (author.annotation_exists) {
        response.push(`📝 Аннотация: /a_an_${author.id}`);
    }

    return response.join('\n');
}


export function formatSequence(sequence: Sequence): string {
    let response = [];

    response.push(`📚 ${sequence.name}`);
    response.push(`/s_${sequence.id}`);

    return response.join('\n');
}
