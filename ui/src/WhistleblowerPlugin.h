#pragma once

#include <QObject>
#include <QWidget>
#include <QtPlugin>

class LogosAPI;
class WhistleblowerBackend;

// Backward-compatible plugin interface for the current Basecamp plugin loader.
class IComponent {
public:
    virtual ~IComponent() = default;
    virtual QWidget* createWidget(LogosAPI* api = nullptr) = 0;
    virtual void     destroyWidget(QWidget* widget) = 0;
};
#define IComponent_iid "com.logos.component.IComponent"
Q_DECLARE_INTERFACE(IComponent, IComponent_iid)

class WhistleblowerPlugin : public QObject, public IComponent {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID IComponent_iid FILE "../metadata.json")
    Q_INTERFACES(IComponent)

public:
    explicit WhistleblowerPlugin(QObject* parent = nullptr);
    ~WhistleblowerPlugin() override;

    Q_INVOKABLE void initLogos(LogosAPI* api);

    QWidget* createWidget(LogosAPI* api = nullptr) override;
    void     destroyWidget(QWidget* widget) override;

private:
    LogosAPI*            m_api     = nullptr;
    WhistleblowerBackend*  m_backend = nullptr;
};
