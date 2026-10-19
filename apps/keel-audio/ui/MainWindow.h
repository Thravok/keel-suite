#pragma once

#include <QJsonObject>
#include <QMainWindow>
#include <QTimer>

class RpcClient;
class QLabel;
class QComboBox;
class QTableWidget;
class QListWidget;
class QSlider;
class QCheckBox;
class QTabWidget;

class MainWindow : public QMainWindow {
  Q_OBJECT
public:
  explicit MainWindow(quint16 port, QWidget *parent = nullptr);

private slots:
  void refreshAll();
  void onConnected(bool on);
  void applyTheme();

private:
  void buildUi();
  void rebuildMatrix(const QJsonObject &state);
  void rebuildDevices(const QJsonValue &devices);
  void rebuildEffects(const QJsonValue &effects);

  quint16 port_;
  RpcClient *rpc_;
  QLabel *status_;
  QComboBox *mainOut_;
  QTableWidget *matrix_;
  QListWidget *channelList_;
  QListWidget *mixList_;
  QListWidget *effectList_;
  QListWidget *waveList_;
  QListWidget *issuesList_;
  QTabWidget *tabs_;
  QTimer refreshTimer_;
  QJsonObject lastState_;
};
