// Standalone test harness — loads WhistleblowerPlugin directly without Basecamp.
// Usage:
//   QML_PATH=ui/qml ./whistleblower_app
//   NSSA_WALLET_HOME_DIR=.scaffold/wallet NSSA_SEQUENCER_URL=http://127.0.0.1:3040 \
//   WHISTLEBLOWER_PROGRAM_ID_HEX=<64-hex> QML_PATH=ui/qml ./whistleblower_app

#include "WhistleblowerPlugin.h"

#include <QApplication>
#include <QMainWindow>

int main(int argc, char* argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("Whistleblower");
    app.setApplicationVersion("0.1.0");

    WhistleblowerPlugin plugin;

    QMainWindow window;
    window.setWindowTitle("Whistleblower — Basecamp module preview");
    window.resize(480, 640);

    QWidget* view = plugin.createWidget(nullptr);
    window.setCentralWidget(view);
    window.show();

    return app.exec();
}
