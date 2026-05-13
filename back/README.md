To host: 
- Create a .env file in the project root
- Add the following env_vars: 
    - GIT_REPO_PATH: where your bare git repo is setup
    - DATABASE_URL: where your postgresql is hosted 
    - JWT_SECRET: Secret for use in jwt token generation
    - SERVE_ADRESS: where to serve the backend
TO DO: 
    - Annotate stuff with utoipa 
    - Finish the pull request section
    - Write some tests with either Postman or utilize cargo test. 
    - Fix any possible bugs? 
    - Trim dependencies, especially for gix. 
