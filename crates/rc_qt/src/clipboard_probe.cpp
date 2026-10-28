// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#include "rc_qt/src/clipboard_probe.h"

#include <QClipboard>
#include <QGuiApplication>
#include <QImage>
#include <QMimeData>
#include <QString>
#include <QUrl>
#include <QVariant>

namespace {

const QMimeData *clipboardData()
{
    const QClipboard *clipboard = QGuiApplication::clipboard();
    return clipboard ? clipboard->mimeData() : nullptr;
}

QString toQString(rust::Str text)
{
    return QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size()));
}

rust::String toRust(const QByteArray &bytes)
{
    return rust::String(bytes.constData(), static_cast<size_t>(bytes.size()));
}

} // namespace

namespace rc_qt {

rust::Vec<rust::String> clipboard_urls()
{
    rust::Vec<rust::String> urls;
    if (const QMimeData *data = clipboardData()) {
        for (const QUrl &url : data->urls())
            urls.push_back(toRust(url.toEncoded()));
    }
    return urls;
}

rust::String clipboard_text()
{
    const QMimeData *data = clipboardData();
    if (!data || !data->hasText())
        return {};
    return toRust(data->text().toUtf8());
}

bool clipboard_has_image()
{
    const QMimeData *data = clipboardData();
    return data && data->hasImage();
}

bool clipboard_save_image(rust::Str path)
{
    const QMimeData *data = clipboardData();
    if (!data || !data->hasImage())
        return false;
    const QImage image = qvariant_cast<QImage>(data->imageData());
    return !image.isNull() && image.save(toQString(path), "PNG");
}

void clipboard_seed(const rust::Vec<rust::String> &paths, rust::Str text, rust::Str image_path)
{
    QClipboard *clipboard = QGuiApplication::clipboard();
    if (!clipboard)
        return;
    auto *data = new QMimeData;
    QList<QUrl> urls;
    for (const rust::String &path : paths)
        urls.append(QUrl::fromLocalFile(QString::fromUtf8(path.data(), static_cast<qsizetype>(path.size()))));
    if (!urls.isEmpty())
        data->setUrls(urls);
    if (!text.empty())
        data->setText(toQString(text));
    if (!image_path.empty())
        data->setImageData(QImage(toQString(image_path)));
    clipboard->setMimeData(data);
}

} // namespace rc_qt
