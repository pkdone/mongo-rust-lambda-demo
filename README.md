# Mongo Rust Lambda Demo

Provides a demo of an [AWS Lambda](https://docs.aws.amazon.com/lambda/latest/dg/) function written in [Rust](https://doc.rust-lang.org/book/) which uses [MongoDB's Rust Driver](https://docs.mongodb.com/drivers/rust/) to connect to and insert records into a remote [MongoDB database](https://docs.mongodb.com/manual/).

AWS provides [specific language Lambda runtimes](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-images.html) for some programming languages and a [custom Lambda runtime](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-walkthrough.html) for other languages (including Rust). This demo uses an AWS custom runtime based on Amazon Linux 2. AWS provides an [open source runtime API](https://aws.amazon.com/blogs/opensource/rust-runtime-for-aws-lambda/) for Rust: the [aws-lambda-rust-runtime](https://github.com/awslabs/aws-lambda-rust-runtime) crate. This crate manages the instantiation of the deployed Rust Lambda function (deployed as a compiled executable called `bootstrap`), and the subsequent invocation of this Lambda function's core logic (called a handler) for each request/event dispatched to it. It also provides an [API](https://docs.rs/lambda_runtime/latest/lambda_runtime/) for the Lambda's main logic to extract a request payload, query the context it is running in and generate a response each time it is invoked. There's no inherent performance impact when using a custom runtime versus a 'standard' runtime. Indeed, the sorts of programming languages that demand a custom Lambda runtime (e.g. C++, Rust) tend to be more efficient anyway.

AWS provides a [documented example for deploying a generic Rust Lambda](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/lambda.html). The project here serves a more specific purpose of giving an example for creating and deploying a Rust Lambda to interact with a MongoDB database. Furthermore, whether developing a Lambda to interact with a MongoDB database in Rust or any other programming language, you should ensure you follow [MongoDB's Best Practices Connecting from AWS Lambda](https://docs.atlas.mongodb.com/best-practices-connecting-from-aws-lambda/).

For a description of what the example Rust Lambda code does, see section [Lambda Rust Code Description](#lambda-rust-code-description) at the base of this page.


## How To Deploy, Test And Monitor

### Prerequisites

 1. Ensure you have an __accessible remote MongoDB cluster__ ([self-managed](https://docs.mongodb.com/manual/installation/) or hosted in the [Atlas](https://www.mongodb.com/cloud/)) DBaaS (which can be leveraging the [free tier](https://docs.atlas.mongodb.com/tutorial/deploy-free-tier-cluster/)) which is network accessible from your client workstation. 

 2. Ensure the MongoDB cluster you are connecting to has a database user available with at least __write privileges__ to the database called `test. If you are using an __Atlas hosted MongoDB database__, you will additionally need to follow the steps in section _Restrict network access to your Atlas cluster_ of the [best practices](https://docs.atlas.mongodb.com/best-practices-connecting-from-aws-lambda/) to enable your subsequently deployed Lambda function __to be able to access the database__.

 3. Install the [AWS Command Line Interface (AWS CLI)](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) (version 2) including the [prerequisites](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-prereqs.html) and then test you have configured the AWS CLI correctly on your workstation by attempting to run the following command from a terminal to list the currently deployed AWS Lambdas in your AWS account region (there may be none):

    ```console
    aws lambda list-functions
    ```

 4. Install the latest version of the [Rust development environment](https://www.rust-lang.org/tools/install), if it isn't already installed, via the __rustup__ utility, including the _rustc_ compiler & the _cargo_ package/build manager. _NOTE:_ If building on Microsoft Windows, first ensure you have Microsoft's [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/) installed (and importantly, when running Microsoft's _build tools_ installer, choose the _C++ build tools_ option). Run the following command from a terminal to install Rust toolchain for the target OS environment the Lambda will execute in (i.e. x86-64 Linux):

    ```console
    rustup target add x86_64-unknown-linux-gnu
    ```

 5. Via the AWS console for your AWS account, [create a new lambda execution IAM role](https://docs.aws.amazon.com/lambda/latest/dg/lambda-intro-execution-role.html#permissions-executionrole-console) in the [IAM section of the console](https://console.aws.amazon.com/iam/home#/roles) using the following steps:
    * Choose _Create role_ and under _Common use cases_, choose _Lambda_.
    * Choose _Next: Permissions_ and under _Under Attach permissions policies_, choose the _AWS managed policies_ __AWSLambdaBasicExecutionRole__ and __AWSXRayDaemonWriteAccess__.
    * Choose _Next: Tags_, choose _Next: Review_ and for _Role name_, enter a new role name with any value, e.g. "jdoe-lambda-role".
    * Choose Create role and once created __make a copy the ARN for the role__ for use later.

### Executable Compilation

 * From a terminal, in this Github project's root folder, run Rust's _cargo_ commands to __1)__ build the project's executable for the target environment, and __2)__ rename the executable to `bootstrap` and bundled into a zip file ready for deployment (as required by AWS Lambda):

```console
cargo build --release --target x86_64-unknown-linux-gnu
rm -f mongo-rust-lambda-demo.zip && cp ./target/x86_64-unknown-linux-gnu/release/mongo-rust-lambda-demo ./bootstrap && zip mongo-rust-lambda-demo.zip bootstrap && rm -f bootstrap
```

### Deployment

 * Run the following AWS CLI command to deploy the Lambda zipped executable, first changing the values of the argument for `--role` to match the __role ARN__ you copied in the Prerequisites, and the __MongoDB URL__ part of the`-environment` argument to match the URL of your own MongoDB environment including database username and password (i.e. replace `mongodb+srv://myuser:mypassword@mycluster.a123z.mongodb.net/`):

```console
aws lambda create-function --function-name mongo-rust-lambda-demo \
  --handler doesnt.matter \
  --zip-file fileb://./mongo-rust-lambda-demo.zip \
  --runtime provided.al2 \
  --role arn:aws:iam::637263836326:role/jdoe-lambda-role \
  --tracing-config Mode=Active \
  --environment Variables="{MONGODB_URL=mongodb+srv://myuser:mypassword@mycluster.a123z.mongodb.net/,RUST_BACKTRACE=1,RUST_LOG='error,warn,info'}"
```

### Testing

 1. Run the following AWS CLI command to invoke your deployed AWS Lambda function and display its response:

```console
aws lambda invoke --function-name mongo-rust-lambda-demo \
  --payload '{"message": "Hi from Jane"}' \
  --cli-binary-format raw-in-base64-out \
  output.json && cat output.json
```

&nbsp;&nbsp;&nbsp;&nbsp;_NOTE 1_: The response from the Rust executable includes a `invocation_count` field which shows how many times the instance of the Lambda has been invoked (there could be more than one when under load). If you run the test command repeatedly in a short space of time, you should see this number increment each time. Suppose you wait more than roughly 15 minutes before invoking the test again. In that case, you will likely see the count reset to one because the AWS Lambda runtime will have destroyed the existing Lambda instance as it had been idle and will have instantiated a new instance upon receiving this later request.

&nbsp;&nbsp;&nbsp;&nbsp;_NOTE 2_: In [real-world environments](https://docs.aws.amazon.com/lambda/latest/dg/lambda-invocation.html), you wouldn't be using the AWS CLI to invoke your Lambda function, and instead, you might be triggering it synchronously via an HTTP API endpoint or asynchronously via an AWS S3 or SNS event for example.

 2. Use the [MongoDB Shell](https://docs.mongodb.com/mongodb-shell/) (`mongosh`) to inspect the document inserted each time the Lambda function was invoked with the previous command by running the following commands (first change the __MongoDB URL__ argument to match the URL of your own MongoDB environment including database username and password):

```console
mongosh "mongodb+srv://myuser:mypassword@mycluster.a123z.mongodb.net/"
```

```javascript
use test
db.lambdalogs.find()
```

### Monitoring

There are a few options for monitoring your deployed Lambda function, including:

 1. From the [Functions page of the Lambda console](https://console.aws.amazon.com/lambda/home#/functions) you can use the _Monitor_ tab to view when your Lambda was invoked, its output and other statistics.
 
 2. Run the following AWS CLI command to _tail_ the emitted log events for the Lambda (then invoke the AWS CLI test again to see logged output):
 
```console
aws logs tail /aws/lambda/mongo-rust-lambda-demo --follow
```

### OPTIONAL: Redeploying

If you make any changes/enhancements to the Rust code, you can rebuild and deploy the new version with the following commands:

```console
cargo build --release --target x86_64-unknown-linux-gnu
rm -f mongo-rust-lambda-demo.zip && cp ./target/x86_64-unknown-linux-gnu/release/mongo-rust-lambda-demo ./bootstrap && zip mongo-rust-lambda-demo.zip bootstrap && rm -f bootstrap

aws lambda update-function-code --function-name mongo-rust-lambda-demo \
  --zip-file fileb://./mongo-rust-lambda-demo.zip
```

## Building From Scratch

For reference, below are some of the commands that can be run to check, compile, build and test the Rust code outside of the AWS Lambda runtime:

```console
# Compile the Rust application to an execetuable runnable on the host workstation
cargo build

# Run the unit tests for the Rust application
cargo test

# Run the application (inside an integration test) on the local worstation (this enables the 
#  majority of the code to be executed outside of the AWS Lambda runtime for rapid testing) 
# IMPORTANT: First change the MongoDB URL listed below to match the URL of your database
RUST_LOG="error,warn,info" MONGODB_URL="mongodb+srv://myuser:mypassword@mycluster.a123z.mongodb.net/" cargo test -- --ignored --show-output

# Run Rust's lint checks to catch common mistakes and suggest where code can be improved:
cargo clippy

# Run Rust's layout format checks to ensure consistent code formatting is used:
cargo fmt -- --check
```

## Lambda Rust Code Description

This project contains a single Rust source file:&nbsp; [main.rs](blob/main/src/main.rs)

Notes about the Rust code:

 * The two key functions are:
    * `main()` - The Lambda initialisation code in this demo instantiates a static reference to a new instance of a MongoDB Driver's client for communicating with the remote database (after first reading the database's URL from the environment variable `MONGODB_URL`). Environment variables are the standard Lambda way to provide context metadata to your Lambda code. This metadata was declared when the AWS CLI command `aws lambda create-function` was used earlier. Finally, the main function declares the handler function (see next sub-bullet) to the Lambda runtime.
    * `handler()` - The Lambda handler code is invoked every time the AWS Lambda receives a request. This uses the AWS Lambda API to read the request JSON payload and some other context data about the Lambda instance. It then invokes some code to insert a log record into a MongoDB database by using the MongoDB Client instantiated earlier in `main()`. Finally, it returns a new JSON payload to the caller.
 * The code declares a Rust structure called `DBLogRecord` used in the function `db_insert_record()` to populate with data ready to be inserted into the MongoDB database collection. The call to the MongoDB Driver's `collection.insert_one()` API automatically transforms the data structure into a MongoDB BSON document to be inserted. The driver _transparently_ uses the Rust serialisation/deserialisation library called `serde` to covert the data structure to a BSON document.
 * Most of the logic for this Lambda is delegated to a function named `process_work()` and functions that then calls. This enables the bulk of the code to be executed outside of the Lambda runtime, directly on your workstation, for rapid prototyping. Specifically, the integration test function `integration_test_execute_full_flow()`, near the end of the source file, invokes the `process_work()` function to execute the main logic end-to-end.
 * The source file also contains a set of unit tests.

