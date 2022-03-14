FROM node:lts-alpine as build-image

WORKDIR /root/app

COPY ./package.json ./
COPY ./tsconfig.json ./
COPY ./src ./src

RUN yarn install --production && yarn build


FROM node:lts-alpine as runtime-image

WORKDIR /root/app

COPY ./package.json ./
COPY ./scripts/healthcheck.js ./

COPY --from=build-image /root/app/build ./build

CMD yarn run run
