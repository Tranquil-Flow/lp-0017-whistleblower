#include "WhistleblowerPlugin.h"
#include "WhistleblowerBackend.h"

#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickWidget>
#include <QUrl>
#include <cstdlib>

WhistleblowerPlugin::WhistleblowerPlugin(QObject* parent) : QObject(parent) {}
WhistleblowerPlugin::~WhistleblowerPlugin() = default;

void WhistleblowerPlugin::initLogos(LogosAPI* api) {
    m_api = api;
}

QWidget* WhistleblowerPlugin::createWidget(LogosAPI* api) {
    if (api) m_api = api;

    if (!m_backend)
        m_backend = new WhistleblowerBackend(m_api, this);

    auto* view = new QQuickWidget();
    view->engine()->rootContext()->setContextProperty("backend", m_backend);
    view->setResizeMode(QQuickWidget::SizeRootObjectToView);

    // Prefer a file-system QML path for development (set QML_PATH=.../ui/qml).
    const char* qmlPath = std::getenv("QML_PATH");
    if (qmlPath) {
        view->setSource(QUrl::fromLocalFile(
            QString::fromUtf8(qmlPath) + "/Main.qml"));
    } else {
        view->setSource(QUrl("qrc:/qml/Main.qml"));
    }

    return view;
}

void WhistleblowerPlugin::destroyWidget(QWidget* widget) {
    delete m_backend;
    m_backend = nullptr;
    delete widget;
}
