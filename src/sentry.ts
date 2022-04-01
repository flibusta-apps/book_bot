import * as Sentry from '@sentry/node';

import env from '@/config';


Sentry.init({
    dsn: env.SENTRY_DSN,
});

export default Sentry;
