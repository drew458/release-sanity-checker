use crate::diff_finder::Difference;
use colored::Colorize;
use tokio::sync::mpsc;

pub struct DifferencesPrinter {
    receiver: mpsc::Receiver<DifferencesPrinterMessage>,
    done_signal: tokio::sync::oneshot::Sender<()>,
}
pub enum DifferencesPrinterMessage {
    PrintDifferences {
        differences: Vec<Difference>,
        request_id: String
    },
}

impl DifferencesPrinter {
    pub fn new(
        receiver: mpsc::Receiver<DifferencesPrinterMessage>,
        done_signal: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        DifferencesPrinter {
            receiver,
            done_signal,
        }
    }
    fn handle_message(&mut self, msg: DifferencesPrinterMessage) {
        match msg {
            DifferencesPrinterMessage::PrintDifferences {
                differences,
                request_id
            } => {
                assert!(!differences.is_empty());

                println!(
                    "\n❌-----------------------------------------------------------------------------------------❌"
                );
                println!(
                    "{}",
                    format!(
                        "Differences detected for request with ID: '{}'",
                        request_id
                    )
                    .yellow()
                );

                for diff in &differences {
                    diff.print();
                }

                println!(
                    "❌-----------------------------------------------------------------------------------------❌"
                );
            }
        }
    }
}

pub async fn run_differences_printer(mut actor: DifferencesPrinter) {
    while let Some(msg) = actor.receiver.recv().await {
        actor.handle_message(msg);
    }

    // Signal we're done
    let _ = actor.done_signal.send(());
}
