import { Context } from "telegraf";
import { AuthorAnnnotation, BookAnnotation } from "./services/book_library";

import { isNormalText } from "./utils";
import { getTextPaginationData } from './keyboard';
import Sentry from '@/sentry';
import { downloadImage } from "./services/downloader";


export function getAnnotationHandler<T extends BookAnnotation | AuthorAnnnotation>(
    annotationGetter: (id: number) => Promise<T>,
    callbackData: string
): (ctx: Context) => Promise<void> {
    return async (ctx: Context) => {
        if (!ctx.message || !('text' in ctx.message)) {
            return;
        }

        const objId = ctx.message.text.split("@")[0].split('_')[2];

        const annotation = await annotationGetter(parseInt(objId));

        if (!annotation.file && !isNormalText(annotation.text)) {
            await ctx.reply("Аннотация недоступна :(");
            return;
        }

        if (annotation.file) {
            const imageData = await downloadImage(annotation.file);

            if (imageData !== null) {
                try {
                    await ctx.telegram.sendPhoto(ctx.message.chat.id, { source: imageData });
                } catch (e) {
                    console.log(e);
                    Sentry.captureException(e);
                }
            }
        }

        if (!isNormalText(annotation.text)) return;

        const data = getTextPaginationData(`${callbackData}${objId}`, annotation.text, 0);

        try {
            await ctx.reply(data.current, {
                parse_mode: "HTML",
                reply_markup: data.keyboard.reply_markup,
            });
        } catch (e) {
            Sentry.captureException(e, {
                extra: {
                    message: data.current,
                    annotation,
                    objId
                }
            })
        }
    }
}
